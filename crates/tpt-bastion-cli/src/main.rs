// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `bastion` — the TPT Bastion command-line interface.
//!
//! The CLI drives the full pipeline: it reads a `.bcl` document, compiles it to
//! IR, builds a dependency-ordered plan (dry-run), and can `apply` that plan
//! through the execution engine. `apply` currently uses the in-tree
//! [`MockProvider`]; real providers are not yet wired into the CLI.
//!
//! State is managed by [`tpt_bastion_state`] (Phase 4): `apply` and `plan` read and
//! write state through a [`StateBackend`] (plaintext or encrypted, lock-aware),
//! and the `state` / `drift` subcommands expose inspection, history, locking,
//! and drift detection / reconciliation.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Duration;
use tpt_bastion_dsl::compile_source;
use tpt_bastion_engine::{
    plan_document_with_state, Executor, MockProvider, PrintingHandler, State as EngineState,
};
use tpt_bastion_events::{run_forever, FileEventSource, WebhookEventSource};
use tpt_bastion_providers::{NativeRuntime, NullProvider, PluginRegistry};
use tpt_bastion_state::{
    backend::StateBackend, drift::detect_drift, file::FileStateBackend,
    reconcile::detect_and_reconcile, reconcile::render_reconcile, State as BastionState,
};

#[derive(Parser)]
#[command(
    name = "bastion",
    version,
    about = "TPT Bastion — next-generation Infrastructure as Code",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a `.bcl` document to the Bastion IR.
    Compile {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output format for the compiled IR.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },

    /// Build an execution plan (dry-run) from a `.bcl` document.
    Plan {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Diff against an existing state file to show updates/deletes.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        /// Print the plan as a JSON object instead of text.
        #[arg(long)]
        json: bool,
    },

    /// Apply an execution plan, creating/updating infrastructure.
    Apply {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// State file to read prior state from and write the new state to.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        /// Apply without an interactive confirmation prompt.
        #[arg(long)]
        auto_approve: bool,
    },

    /// Inspect and manage Bastion native state (Phase 4).
    State {
        /// Path to the state file.
        #[arg(long, value_name = "PATH")]
        file: PathBuf,

        /// Environment variable holding the encryption passphrase (omit for plaintext).
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        #[command(subcommand)]
        action: StateAction,
    },

    /// Detect and reconcile drift between a document and recorded state (Phase 4).
    Drift {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// State file holding recorded infrastructure.
        #[arg(long, value_name = "PATH")]
        state: PathBuf,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        /// Print the reconciliation plan that would bring state back in line.
        #[arg(long)]
        reconcile: bool,
    },

    /// Manage provider plugins (Phase 5).
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },

    /// React to external events and reconcile infrastructure (Phase 7).
    Events {
        #[command(subcommand)]
        action: EventsAction,
    },

    /// Serve a local web dashboard for this document (compile/plan/state/drift).
    Ui {
        /// Path to the `.bcl` document to visualize. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// State file for plan diffs, state inspection, and drift.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,

        /// TCP port to bind on 127.0.0.1.
        #[arg(long, default_value_t = 7474)]
        port: u16,

        /// Bearer token required on every `/api/*` request (loopback only).
        #[arg(long, value_name = "VAR")]
        token_env: Option<String>,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        /// Expose the mutating `apply` action in the dashboard.
        #[arg(long)]
        allow_apply: bool,
    },

    /// Check a `.bcl` document: compile it and verify its dependency graph.
    Validate {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    /// Print the resource dependency graph as Graphviz DOT or Mermaid.
    Graph {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output format.
        #[arg(long, value_enum, default_value_t = GraphFormat::Dot)]
        format: GraphFormat,
    },

    /// Format `.bcl` files in canonical style (indentation, blank lines).
    Fmt {
        /// Files to format. Use `-` to read stdin and write stdout.
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,

        /// Do not write; exit non-zero if any file would change.
        #[arg(long)]
        check: bool,
    },

    /// Scaffold a starter project (`main.bcl`) in a directory.
    Init {
        /// Directory to create the project in (default: current directory).
        #[arg(value_name = "DIR", default_value = ".")]
        dir: PathBuf,
    },

    /// Print the document's outputs, resolved against recorded state.
    Output {
        /// Path to the `.bcl` source file. Use `-` to read from stdin.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// State file holding the applied resources.
        #[arg(long, value_name = "PATH")]
        state: PathBuf,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,

        /// Print only the output with this name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,

        /// Print all outputs as a JSON object.
        #[arg(long)]
        json: bool,
    },

    /// Check the environment, a document, and a state file for common problems.
    Doctor {
        /// Optional `.bcl` document to compile and validate.
        #[arg(long, value_name = "FILE")]
        file: Option<PathBuf>,

        /// Optional state file to check for readability and integrity.
        #[arg(long, value_name = "PATH")]
        state: Option<PathBuf>,

        /// Environment variable holding the state encryption passphrase.
        #[arg(long, value_name = "VAR")]
        passphrase_env: Option<String>,
    },

    /// Generate a shell completion script on stdout.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

/// Output formats for `bastion graph`.
#[derive(Clone, Copy, ValueEnum)]
enum GraphFormat {
    Dot,
    Mermaid,
}

/// What to do via the `events` subcommand.
#[derive(Subcommand)]
enum EventsAction {
    /// Watch files and re-plan/re-apply when they change.
    Watch {
        /// Path to the `.bcl` document to drive on each event.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Additional paths to watch (defaults to the document itself).
        #[arg(long, value_name = "PATH")]
        watch: Vec<PathBuf>,

        /// Run `apply --auto-approve` instead of `plan` on each event.
        #[arg(long)]
        apply: bool,
    },
    /// Accept webhook POSTs and re-plan/re-apply when they arrive.
    Webhook {
        /// Path to the `.bcl` document to drive on each event.
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// TCP port to listen on (binds 127.0.0.1).
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Run `apply --auto-approve` instead of `plan` on each event.
        #[arg(long)]
        apply: bool,

        /// Environment variable holding the shared secret required on every
        /// inbound request. Required when `--apply` is set (fail-closed).
        #[arg(long, value_name = "VAR")]
        secret_env: Option<String>,
    },
}

/// What to do with a state file via the `state` subcommand.
#[derive(Subcommand)]
enum StateAction {
    /// Print the current state contents as JSON.
    Show,
    /// Print the append-only version history.
    History,
    /// Show the current lock holder, if any.
    LockStatus,
}

/// What to do via the `provider` subcommand.
#[derive(Subcommand)]
enum ProviderAction {
    /// Emit a starter provider crate.
    Scaffold {
        /// Provider name (e.g. `acme`).
        #[arg(value_name = "NAME")]
        name: String,

        /// Output directory for the generated crate.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// List providers registered in the default runtime.
    List,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Format {
    /// Pretty-printed JSON (default).
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Compile { file, format } => cmd_compile(&file, format),
        Command::Plan {
            file,
            state,
            passphrase_env,
            json,
        } => cmd_plan(&file, state.as_ref(), passphrase_env.as_deref(), json),
        Command::Apply {
            file,
            state,
            passphrase_env,
            auto_approve,
        } => cmd_apply(
            &file,
            state.as_ref(),
            passphrase_env.as_deref(),
            auto_approve,
        ),
        Command::State {
            file,
            passphrase_env,
            action,
        } => cmd_state(&file, passphrase_env.as_deref(), &action),
        Command::Drift {
            file,
            state,
            passphrase_env,
            reconcile,
        } => cmd_drift(&file, &state, passphrase_env.as_deref(), reconcile),
        Command::Provider { action } => cmd_provider(&action),
        Command::Events { action } => cmd_events(&action),
        Command::Validate { file } => cmd_validate(&file),
        Command::Graph { file, format } => cmd_graph(&file, format),
        Command::Init { dir } => cmd_init(&dir),
        Command::Fmt { files, check } => cmd_fmt(&files, check),
        Command::Output {
            file,
            state,
            passphrase_env,
            name,
            json,
        } => cmd_output(
            &file,
            &state,
            passphrase_env.as_deref(),
            name.as_deref(),
            json,
        ),
        Command::Doctor {
            file,
            state,
            passphrase_env,
        } => cmd_doctor(file.as_ref(), state.as_ref(), passphrase_env.as_deref()),
        Command::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "bastion",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Command::Ui {
            file,
            state,
            port,
            token_env,
            passphrase_env,
            allow_apply,
        } => cmd_ui(
            &file,
            state.as_ref(),
            port,
            token_env.as_deref(),
            passphrase_env.as_deref(),
            allow_apply,
        ),
    }
}

/// Run `bastion plan` (or `apply --auto-approve`) for `file` in response to an
/// external event, printing the result.
fn run_bastion_on_event(file: &PathBuf, apply: bool) -> Result<()> {
    let exe = std::env::current_exe().context("resolving current executable")?;
    let mut cmd = std::process::Command::new(exe);
    if apply {
        let state = file.with_extension("bclstate");
        cmd.args(["apply", "--auto-approve", "--state"]).arg(&state);
    } else {
        cmd.arg("plan");
    }
    cmd.arg(file);
    let status = cmd
        .status()
        .with_context(|| format!("running bastion for {}", file.display()))?;
    if !status.success() {
        eprintln!("  bastion exited with status {status}");
    }
    Ok(())
}

fn cmd_events(action: &EventsAction) -> Result<()> {
    match action {
        EventsAction::Watch { file, watch, apply } => {
            let paths = if watch.is_empty() {
                vec![file.clone()]
            } else {
                watch.clone()
            };
            let watched = paths.len();
            let mut source = FileEventSource::new(paths, Duration::from_secs(1));
            println!("Watching {watched} path(s); Ctrl-C to stop.");
            run_forever(&mut source, |ev| {
                println!("[event] {ev}");
                if let Err(e) = run_bastion_on_event(file, *apply) {
                    eprintln!("  error: {e:#}");
                }
            });
        }
        EventsAction::Webhook {
            file,
            port,
            apply,
            secret_env,
        } => {
            // Fail closed: a webhook that can trigger applies must be authenticated.
            let secret = if *apply {
                let var = secret_env.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "refusing to start an apply-on-webhook listener without --secret-env; \
                         set it to an env var holding the shared token"
                    )
                })?;
                Some(
                    std::env::var(var)
                        .with_context(|| format!("webhook secret env var `{var}` is not set"))?,
                )
            } else {
                secret_env.as_deref().and_then(|v| std::env::var(v).ok())
            };
            let addr = format!("127.0.0.1:{port}");
            let authenticated = secret.is_some();
            let source = WebhookEventSource::bind_with_secret(&addr, secret)
                .map_err(|e| anyhow::anyhow!("bind: {e}"))?;
            println!(
                "Listening for webhooks on {addr} ({}{}); Ctrl-C to stop.",
                if authenticated {
                    "authenticated"
                } else {
                    "open"
                },
                if *apply { ", apply mode" } else { "" }
            );
            let mut source = source;
            run_forever(&mut source, |ev| {
                println!("[event] {ev}");
                if let Err(e) = run_bastion_on_event(file, *apply) {
                    eprintln!("  error: {e:#}");
                }
            });
        }
    }
}

fn cmd_ui(
    file: &std::path::Path,
    state: Option<&PathBuf>,
    port: u16,
    token_env: Option<&str>,
    passphrase_env: Option<&str>,
    allow_apply: bool,
) -> Result<()> {
    // Resolve the document path; `-` (stdin) is not supported by the long-lived
    // server, which needs a stable file to poll.
    if file.as_os_str() == "-" {
        anyhow::bail!("`bastion ui` requires a file path, not stdin (`-`)");
    }
    let token = match token_env {
        Some(var) => Some(
            std::env::var(var).with_context(|| format!("ui token env var `{var}` is not set"))?,
        ),
        None => None,
    };
    let config = tpt_bastion_ui::UiConfig {
        file: file.to_path_buf(),
        state: state.cloned(),
        port,
        token,
        passphrase_env: passphrase_env.map(|s| s.to_string()),
        allow_apply,
    };
    tpt_bastion_ui::serve(config)
}

/// `bastion validate`: compile the document and build its dependency graph,
/// which also rejects cycles and references to unknown objects.
fn cmd_validate(file: &PathBuf) -> Result<()> {
    let source = read_source(file)?;
    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let graph = tpt_bastion_engine::DepGraph::build(&doc)
        .map_err(|e| anyhow::anyhow!("dependency check failed: {e}"))?;
    let order = graph
        .order()
        .map_err(|e| anyhow::anyhow!("dependency check failed: {e}"))?;
    println!(
        "ok: {} node(s), {} resource(s), {} output(s) in dependency order",
        order.len(),
        doc.resources.len(),
        doc.outputs.len()
    );
    Ok(())
}

/// `bastion graph`: render the dependency graph. Edges point from a node to
/// the node that depends on it, so the diagram reads in creation order.
fn cmd_graph(file: &PathBuf, format: GraphFormat) -> Result<()> {
    let source = read_source(file)?;
    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let graph = tpt_bastion_engine::DepGraph::build(&doc)
        .map_err(|e| anyhow::anyhow!("dependency check failed: {e}"))?;
    let mut nodes = graph.nodes.clone();
    nodes.sort();

    let mut edges: Vec<(String, String)> = Vec::new();
    for node in &nodes {
        if let Some(deps) = graph.deps.get(node) {
            for dep in deps {
                edges.push((dep.clone(), node.clone()));
            }
        }
    }
    edges.sort();

    match format {
        GraphFormat::Dot => {
            println!("digraph bastion {{");
            println!("  rankdir=LR;");
            for node in &nodes {
                println!("  \"{}\";", node.replace('"', "\\\""));
            }
            for (from, to) in &edges {
                println!(
                    "  \"{}\" -> \"{}\";",
                    from.replace('"', "\\\""),
                    to.replace('"', "\\\"")
                );
            }
            println!("}}");
        }
        GraphFormat::Mermaid => {
            println!("graph LR");
            // Mermaid ids cannot contain dots, so map each node to an index.
            let index: std::collections::HashMap<&String, usize> =
                nodes.iter().enumerate().map(|(i, n)| (n, i)).collect();
            for node in &nodes {
                println!("  n{}[\"{}\"]", index[node], node.replace('"', "'"));
            }
            for (from, to) in &edges {
                println!("  n{} --> n{}", index[from], index[to]);
            }
        }
    }
    Ok(())
}

/// Resolve a document value against the document's variables and the recorded
/// state. References read the attribute of the applied resource they name.
fn resolve_output_value(
    value: &tpt_bastion_ir::Value,
    doc: &tpt_bastion_ir::Document,
    state: &BastionState,
) -> Result<tpt_bastion_ir::Value> {
    use tpt_bastion_ir::Value;
    Ok(match value {
        Value::Reference { base, path } => {
            if let Some(var) = doc.variables.iter().find(|v| &v.name == base) {
                anyhow::ensure!(
                    path.is_empty(),
                    "variable `{base}` cannot be indexed in an output"
                );
                return var
                    .default
                    .clone()
                    .with_context(|| format!("variable `{base}` has no default value"));
            }
            let res = doc
                .resources
                .iter()
                .find(|r| &r.name == base)
                .with_context(|| format!("output references unknown object `{base}`"))?;
            let applied = state
                .get(&res.id)
                .with_context(|| format!("`{}` is not in state; run apply first", res.id))?;
            let (first, rest) = path
                .split_first()
                .with_context(|| format!("reference `{base}` names no attribute"))?;
            let mut cur = applied
                .attributes
                .get(first)
                .cloned()
                .with_context(|| format!("`{}` has no attribute `{first}`", res.id))?;
            for key in rest {
                cur = match cur {
                    Value::Map(mut m) => m
                        .remove(key)
                        .with_context(|| format!("no key `{key}` in `{}`", res.id))?,
                    _ => anyhow::bail!("cannot read `{key}` from a non-map value in `{}`", res.id),
                };
            }
            cur
        }
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|v| resolve_output_value(v, doc, state))
                .collect::<Result<_>>()?,
        ),
        Value::Map(m) => Value::Map(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), resolve_output_value(v, doc, state)?)))
                .collect::<Result<_>>()?,
        ),
        other => other.clone(),
    })
}

/// Convert a resolved IR value to plain JSON (not the IR's tagged encoding).
fn to_plain_json(value: &tpt_bastion_ir::Value) -> serde_json::Value {
    use tpt_bastion_ir::Value;
    match value {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Null | Value::Reference { .. } => serde_json::Value::Null,
        Value::List(items) => serde_json::Value::Array(items.iter().map(to_plain_json).collect()),
        Value::Map(m) => serde_json::Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), to_plain_json(v)))
                .collect(),
        ),
    }
}

/// Render a resolved value for plain output: strings unquoted, everything
/// else as compact JSON.
fn render_plain(value: &tpt_bastion_ir::Value) -> String {
    match value {
        tpt_bastion_ir::Value::String(s) => s.clone(),
        other => to_plain_json(other).to_string(),
    }
}

/// `bastion output`: resolve each output against the recorded state.
fn cmd_output(
    file: &PathBuf,
    state_path: &PathBuf,
    passphrase_env: Option<&str>,
    name: Option<&str>,
    json: bool,
) -> Result<()> {
    let source = read_source(file)?;
    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let backend = build_backend(state_path, passphrase_env)?;
    let state = load_prior(&backend)?;

    let selected: Vec<_> = doc
        .outputs
        .iter()
        .filter(|o| match name {
            Some(n) => o.name == n,
            None => true,
        })
        .collect();
    if let Some(n) = name {
        anyhow::ensure!(!selected.is_empty(), "no output named `{n}`");
    }

    let mut resolved = std::collections::BTreeMap::new();
    for output in selected {
        let value = resolve_output_value(&output.value, &doc, &state)
            .with_context(|| format!("resolving output `{}`", output.name))?;
        resolved.insert(output.name.clone(), value);
    }

    if json {
        let obj: serde_json::Map<String, serde_json::Value> = resolved
            .iter()
            .map(|(k, v)| (k.clone(), to_plain_json(v)))
            .collect();
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else if name.is_some() {
        // A single named output prints just its value, for scripting.
        for value in resolved.values() {
            println!("{}", render_plain(value));
        }
    } else {
        for (key, value) in &resolved {
            println!("{key} = {}", render_plain(value));
        }
    }
    Ok(())
}

/// Whether a directory accepts new files (probed with a temporary file).
fn dir_writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(format!(".bastion-doctor-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// `bastion doctor`: run environment, document, and state checks. Prints one
/// line per check and fails if any check fails.
fn cmd_doctor(
    file: Option<&PathBuf>,
    state_path: Option<&PathBuf>,
    passphrase_env: Option<&str>,
) -> Result<()> {
    let mut failures = 0usize;
    let mut report = |ok: bool, label: &str, detail: String| {
        let tag = if ok { "ok  " } else { "FAIL" };
        println!("[{tag}] {label}: {detail}");
        if !ok {
            failures += 1;
        }
    };

    report(
        true,
        "version",
        format!("bastion {}", env!("CARGO_PKG_VERSION")),
    );
    report(
        true,
        "platform",
        format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH),
    );
    let ir_ok = tpt_bastion_ir::is_compatible(tpt_bastion_ir::IR_VERSION);
    report(
        ir_ok,
        "IR format",
        format!("version {}", tpt_bastion_ir::IR_VERSION),
    );
    let tmp = std::env::temp_dir();
    report(
        dir_writable(&tmp),
        "temp directory",
        format!("{} writable", tmp.display()),
    );

    if let Some(file) = file {
        match read_source(file).and_then(|src| {
            compile_source(&src).map_err(|e| anyhow::anyhow!("compile failed: {e}"))
        }) {
            Ok(doc) => {
                report(true, "document", format!("{} compiles", file.display()));
                match tpt_bastion_engine::DepGraph::build(&doc).and_then(|g| g.order().map(|_| ()))
                {
                    Ok(()) => report(
                        true,
                        "dependency graph",
                        "acyclic, all references known".into(),
                    ),
                    Err(e) => report(false, "dependency graph", e.to_string()),
                }
            }
            Err(e) => report(false, "document", format!("{e:#}")),
        }
    }

    if let Some(state_path) = state_path {
        let dir = match state_path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let dir_ok = dir.is_dir() && dir_writable(&dir);
        report(
            dir_ok,
            "state directory",
            if !dir.is_dir() {
                format!("{} does not exist", dir.display())
            } else {
                format!("{} writable (needed for locking)", dir.display())
            },
        );
        let env_ok = match passphrase_env {
            Some(v) => std::env::var(v).is_ok(),
            None => true,
        };
        report(
            env_ok,
            "passphrase",
            match passphrase_env {
                Some(v) if env_ok => format!("${v} is set"),
                Some(v) => format!("${v} is not set"),
                None => "not required (plaintext state)".into(),
            },
        );
        if env_ok {
            match build_backend(state_path, passphrase_env).and_then(|b| load_prior(&b)) {
                Ok(state) => report(
                    true,
                    "state",
                    format!(
                        "{} readable, {} resource(s)",
                        state_path.display(),
                        state.resources.len()
                    ),
                ),
                Err(e) => report(false, "state", format!("{e:#}")),
            }
        }
    }

    if failures > 0 {
        anyhow::bail!("{failures} check(s) failed");
    }
    println!("all checks passed");
    Ok(())
}

/// `bastion fmt`: format documents in place, or check them with `--check`.
///
/// A file is never written unless the formatted text compiles to the same IR
/// as the original. Formatting changes layout only, so a mismatch means a bug
/// in the formatter; the file is left alone and the error is reported.
fn cmd_fmt(files: &[PathBuf], check: bool) -> Result<()> {
    anyhow::ensure!(
        !files.is_empty(),
        "no input given; pass one or more .bcl files, or `-` for stdin"
    );

    let mut would_change = 0usize;
    for file in files {
        let stdin_mode = file.as_os_str() == "-";
        let original = read_source(file)?;
        let formatted = tpt_bastion_dsl::format::format_source(&original);

        // Layout must not change meaning. Compare IR when the original compiles.
        if let Ok(before) = compile_source(&original) {
            let after = compile_source(&formatted).map_err(|e| {
                anyhow::anyhow!("{}: formatted output does not compile: {e}", file.display())
            })?;
            anyhow::ensure!(
                before == after,
                "{}: formatting changed the compiled IR; file left unchanged",
                file.display()
            );
        }

        if stdin_mode {
            anyhow::ensure!(!check, "--check cannot be used with stdin");
            print!("{formatted}");
            continue;
        }
        if formatted == original.replace("\r\n", "\n") {
            continue;
        }
        if check {
            println!("would reformat {}", file.display());
            would_change += 1;
        } else {
            std::fs::write(file, &formatted)
                .with_context(|| format!("writing {}", file.display()))?;
            println!("formatted {}", file.display());
        }
    }

    if check && would_change > 0 {
        anyhow::bail!("{would_change} file(s) need formatting");
    }
    Ok(())
}

/// Starter document written by `bastion init`.
const STARTER_BCL: &str = r#"// SPDX-License-Identifier: Apache-2.0 OR MIT
// Starter Bastion document. See `bastion validate` and `bastion plan`.

config {
    name = "my-project"
}

resource aws_s3_bucket main {
    bucket = "my-project-bucket"
    acl = "private"
}

output bucket_name {
    value = main.bucket
}
"#;

/// `bastion init`: write a starter `main.bcl`, refusing to overwrite one.
fn cmd_init(dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let target = dir.join("main.bcl");
    if target.exists() {
        anyhow::bail!("{} already exists; not overwriting", target.display());
    }
    std::fs::write(&target, STARTER_BCL)
        .with_context(|| format!("writing {}", target.display()))?;
    println!("created {}", target.display());
    println!("next: bastion validate {}", target.display());
    Ok(())
}

fn read_source(file: &PathBuf) -> Result<String> {
    if file.as_os_str() == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading source from stdin")?;
        Ok(buf)
    } else {
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))
    }
}

/// Build a state backend for `path`, using a passphrase from the environment
/// variable `passphrase_env` when provided (encrypted storage) or plaintext
/// otherwise.
fn build_backend(path: &PathBuf, passphrase_env: Option<&str>) -> Result<FileStateBackend> {
    match passphrase_env {
        Some(var) => {
            let pass = std::env::var(var)
                .with_context(|| format!("state passphrase env var `{var}` is not set"))?;
            Ok(FileStateBackend::encrypted(path, pass))
        }
        None => Ok(FileStateBackend::plain(path)),
    }
}

/// Load prior state from a backend if a state file exists, else an empty state.
fn load_prior(backend: &FileStateBackend) -> Result<BastionState> {
    backend
        .load_state()
        .context("loading prior state from backend")
}

fn cmd_compile(file: &PathBuf, format: Format) -> Result<()> {
    let source = read_source(file)?;

    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;

    match format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&doc).context("serializing IR to JSON")?;
            println!("{json}");
        }
    }
    Ok(())
}

/// Machine-readable form of a plan for `bastion plan --json`.
fn plan_to_json(plan: &tpt_bastion_engine::Plan, prior_count: usize) -> serde_json::Value {
    let actions: Vec<serde_json::Value> = plan
        .actions
        .iter()
        .map(|a| {
            let changes: Vec<serde_json::Value> = a
                .changes
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "before": c.before.as_ref().map(to_plain_json),
                        "after": c.after.as_ref().map(to_plain_json),
                        "known_after_apply": c.known_after_apply,
                    })
                })
                .collect();
            serde_json::json!({
                "action": a.kind.verb(),
                "id": a.id,
                "type": a.type_name,
                "changes": changes,
            })
        })
        .collect();
    serde_json::json!({
        "actions": actions,
        "summary": {
            "create": plan.to_create,
            "update": plan.to_update,
            "delete": plan.to_delete,
            "unchanged": plan.unchanged,
        },
        "prior_resources": prior_count,
    })
}

fn cmd_plan(
    file: &PathBuf,
    state: Option<&PathBuf>,
    passphrase_env: Option<&str>,
    json: bool,
) -> Result<()> {
    let source = read_source(file)?;

    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let prior = match state {
        Some(p) => {
            let backend = build_backend(p, passphrase_env)?;
            let s: BastionState = load_prior(&backend)?;
            EngineState::from(&s)
        }
        None => EngineState::new(),
    };
    let plan =
        plan_document_with_state(&doc, &prior).map_err(|e| anyhow::anyhow!("plan failed: {e}"))?;

    if json {
        let value = plan_to_json(&plan, prior.resources.len());
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("{}", render_plan(&plan));
    if prior.resources.is_empty() {
        println!("Note: no prior state; every resource plans as a create.");
    } else {
        println!(
            "Note: diffed against {} prior resource(s).",
            prior.resources.len()
        );
    }
    Ok(())
}

fn cmd_apply(
    file: &PathBuf,
    state: Option<&PathBuf>,
    passphrase_env: Option<&str>,
    auto_approve: bool,
) -> Result<()> {
    let source = read_source(file)?;

    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;
    let prior: BastionState = match state {
        Some(p) => {
            let backend = build_backend(p, passphrase_env)?;
            load_prior(&backend)?
        }
        None => BastionState::new(),
    };
    let prior_eng: EngineState = (&prior).into();
    let plan = plan_document_with_state(&doc, &prior_eng)
        .map_err(|e| anyhow::anyhow!("plan failed: {e}"))?;

    println!("{}", render_plan(&plan));

    if !auto_approve {
        anyhow::bail!(
            "this is a mutating operation; re-run with --auto-approve to apply, \
             or pass --state to persist the resulting state"
        );
    }

    // Acquire the state lock so concurrent `bastion` runs cannot clobber it.
    let _guard = if let Some(p) = state {
        let backend = build_backend(p, passphrase_env)?;
        let holder = format!("bastion-cli-{}", process::id());
        Some(
            backend
                .lock(&holder, Duration::from_secs(300))
                .map_err(|e| anyhow::anyhow!("could not lock state: {e}"))?,
        )
    } else {
        None
    };

    let executor = Executor::new(MockProvider, Box::new(PrintingHandler::default()));
    let new_eng = executor
        .apply(&doc, &plan, &prior_eng)
        .map_err(|e| anyhow::anyhow!("apply failed: {e}"))?;

    if let Some(p) = state {
        let backend = build_backend(p, passphrase_env)?;
        let new_state: BastionState = new_eng.into();
        backend
            .save_state(&new_state)
            .with_context(|| format!("saving state to {}", p.display()))?;
        println!("State written to {}", p.display());
    }
    Ok(())
}

fn cmd_state(file: &PathBuf, passphrase_env: Option<&str>, action: &StateAction) -> Result<()> {
    let backend = build_backend(file, passphrase_env)?;
    match action {
        StateAction::Show => {
            let state = backend.load_state().context("loading state")?;
            let json = serde_json::to_string_pretty(&state).context("serializing state to JSON")?;
            println!("{json}");
        }
        StateAction::History => {
            let history = backend.read_history().context("reading history")?;
            if history.is_empty() {
                println!("No history recorded for this state yet.");
            } else {
                println!("History ({} revision(s)):", history.len());
                for entry in &history.entries {
                    println!(
                        "  {:>3}. {} [{}] +{} ~{} -{} (total {})",
                        entry.index,
                        entry.timestamp,
                        &entry.state_hash[..8.min(entry.state_hash.len())],
                        entry.summary.created,
                        entry.summary.updated,
                        entry.summary.deleted,
                        entry.summary.total,
                    );
                }
            }
        }
        StateAction::LockStatus => {
            let lock_path = lock_file_for(file);
            match tpt_bastion_state::lock::read_lock_file(&lock_path)? {
                None => println!("State is not locked."),
                Some(info) => {
                    let stale = tpt_bastion_state::lock::is_expired(&info);
                    println!(
                        "Locked by `{}` (acquired {}, expires {}; {})",
                        info.holder,
                        info.acquired_at,
                        info.expires_at,
                        if stale { "stale" } else { "live" }
                    );
                }
            }
        }
    }
    Ok(())
}

fn cmd_drift(
    file: &PathBuf,
    state: &PathBuf,
    passphrase_env: Option<&str>,
    reconcile: bool,
) -> Result<()> {
    let source = read_source(file)?;
    let doc = compile_source(&source).map_err(|e| anyhow::anyhow!("compile failed: {e}"))?;

    let backend = build_backend(state, passphrase_env)?;
    let recorded = backend.load_state().context("loading state")?;

    let report = detect_drift(&doc, &recorded);
    println!("{}", report.render());

    if reconcile && !report.is_clean() {
        let (_report, plan) = detect_and_reconcile(&doc, &recorded);
        println!("\nReconciliation plan:");
        println!("{}", render_reconcile(&plan));
        println!("Run `bastion apply --auto-approve --state <file>` to apply it.");
    }
    Ok(())
}

/// Mirror the lock-file path derivation used by [`FileStateBackend`].
fn lock_file_for(state_path: &std::path::Path) -> PathBuf {
    let base = state_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "state".into());
    let parent = state_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    parent.join(format!("{base}.lock"))
}

/// Build the CLI's default provider registry (native runtime + built-in
/// `null` provider), used by `bastion provider list`.
fn default_registry() -> PluginRegistry {
    let runtime = NativeRuntime::new();
    runtime.register(Arc::new(NullProvider::new()));
    let registry = PluginRegistry::new();
    registry.register_runtime("native", Arc::new(runtime));
    registry
}

fn cmd_provider(action: &ProviderAction) -> Result<()> {
    match action {
        ProviderAction::Scaffold { name, out } => {
            let dir = out
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let out = tpt_bastion_providers::scaffold::scaffold_provider(name, &dir)
                .with_context(|| format!("scaffolding provider `{name}`"))?;
            println!("Scaffolded provider `{name}` at {}", out.dir.display());
            for f in &out.files {
                println!("  created {}", f.display());
            }
            println!("Next: cargobuild the crate, then load its binary with SubprocessRuntime.");
        }
        ProviderAction::List => {
            let registry = default_registry();
            let names = registry.names();
            if names.is_empty() {
                println!("No providers registered.");
            } else {
                println!("Registered providers ({}):", names.len());
                for n in names {
                    println!("  - {n}");
                }
            }
        }
    }
    Ok(())
}

/// Plan text for the terminal: colored only when stdout is a terminal and
/// `NO_COLOR` is unset, so piped output stays plain.
fn render_plan(plan: &tpt_bastion_engine::Plan) -> String {
    use std::io::IsTerminal;
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    if color {
        plan.render_colored()
    } else {
        plan.render()
    }
}
