// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `bastion` — the TPT Bastion command-line interface.
//!
//! The CLI drives the full pipeline: it reads a `.bcl` document, compiles it to
//! IR, builds a dependency-ordered plan (dry-run), and can `apply` that plan
//! through the execution engine. `apply` uses the in-tree [`MockProvider`] until
//! the Wasm provider runtime (Phase 6) lands.
//!
//! State is managed by [`bastion_state`] (Phase 4): `apply` and `plan` read and
//! write state through a [`StateBackend`] (plaintext or encrypted, lock-aware),
//! and the `state` / `drift` subcommands expose inspection, history, locking,
//! and drift detection / reconciliation.

use anyhow::{Context, Result};
use bastion_dsl::compile_source;
use bastion_engine::{
    plan_document_with_state, Executor, MockProvider, PrintingHandler, State as EngineState,
};
use bastion_events::{run_forever, FileEventSource, WebhookEventSource};
use bastion_providers::{NativeRuntime, NullProvider, PluginRegistry};
use bastion_state::{
    backend::StateBackend, drift::detect_drift, file::FileStateBackend,
    reconcile::detect_and_reconcile, reconcile::render_reconcile, State as BastionState,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Duration;

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
        } => cmd_plan(&file, state.as_ref(), passphrase_env.as_deref()),
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
        EventsAction::Webhook { file, port, apply } => {
            let addr = format!("127.0.0.1:{port}");
            let source =
                WebhookEventSource::bind(&addr).map_err(|e| anyhow::anyhow!("bind: {e}"))?;
            println!("Listening for webhooks on {addr}; Ctrl-C to stop.");
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

fn cmd_plan(file: &PathBuf, state: Option<&PathBuf>, passphrase_env: Option<&str>) -> Result<()> {
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

    println!("{}", plan.render());
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

    println!("{}", plan.render());

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
            match bastion_state::lock::read_lock_file(&lock_path)? {
                None => println!("State is not locked."),
                Some(info) => {
                    let stale = bastion_state::lock::is_expired(&info);
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
            let out = bastion_providers::scaffold::scaffold_provider(name, &dir)
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
