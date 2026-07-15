// SPDX-License-Identifier: Apache-2.0 OR MIT

//! JSON API handlers for the dashboard.
//!
//! Each handler reuses the core crates to compute its result and returns a
//! `serde_json::Value`. The engine/state types themselves are not all
//! `Serialize`, so we project them into small view structs here.

use crate::UiConfig;
use bastion_dsl::compile_source;
use bastion_engine::{plan_document_with_state, EngineState};
use bastion_ir::Value;
use bastion_providers::{NativeRuntime, NullProvider, PluginRegistry};
use bastion_state::{
    backend::StateBackend, detect_drift, file::FileStateBackend, State as BastionState,
};
use serde::Serialize;
use serde_json::{json, Value as Json};
use std::path::PathBuf;
use std::sync::Arc;

/// Read the configured `.bcl` document's source text.
fn read_source(file: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("reading {}: {e}", file.display()))
}

/// Build a state backend for the configured state path, applying the
/// passphrase from `passphrase_env` when set.
fn build_backend(config: &UiConfig) -> Result<Option<FileStateBackend>, String> {
    let Some(state) = &config.state else {
        return Ok(None);
    };
    let pass = match &config.passphrase_env {
        Some(var) => Some(
            std::env::var(var).map_err(|_| format!("passphrase env var `{var}` is not set"))?,
        ),
        None => None,
    };
    Ok(Some(match pass {
        Some(p) => FileStateBackend::encrypted(state, p),
        None => FileStateBackend::plain(state),
    }))
}

/// Compile the document to IR.
pub fn compile(config: &UiConfig) -> Result<Json, String> {
    let src = read_source(&config.file)?;
    let doc = compile_source(&src).map_err(|e| format!("compile failed: {e}"))?;
    // `bastion_ir::Document` is `Serialize`.
    serde_json::to_value(&doc).map_err(|e| e.to_string())
}

/// Build the execution plan (diffed against prior state when a state file is set).
pub fn plan(config: &UiConfig) -> Result<Json, String> {
    let src = read_source(&config.file)?;
    let doc = compile_source(&src).map_err(|e| format!("compile failed: {e}"))?;
    let prior_eng: EngineState = match build_backend(config)? {
        Some(backend) => {
            let state: BastionState = backend.load_state().map_err(|e| e.to_string())?;
            (&state).into()
        }
        None => EngineState::new(),
    };
    let plan = plan_document_with_state(&doc, &prior_eng).map_err(|e| format!("plan failed: {e}"))?;
    Ok(serde_json::to_value(PlanView::from(&plan)).map_err(|e| e.to_string())?)
}

/// Load and return the recorded state.
pub fn state(config: &UiConfig) -> Result<Json, String> {
    match build_backend(config)? {
        Some(backend) => {
            let s: BastionState = backend.load_state().map_err(|e| e.to_string())?;
            serde_json::to_value(&s).map_err(|e| e.to_string())
        }
        None => Ok(json!({ "note": "no state file configured" })),
    }
}

/// Return the append-only state history.
pub fn history(config: &UiConfig) -> Result<Json, String> {
    match build_backend(config)? {
        Some(backend) => {
            let h = backend.read_history().map_err(|e| e.to_string())?;
            let text = h.to_json().map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| e.to_string())
        }
        None => Ok(json!({ "note": "no state file configured" })),
    }
}

/// Detect drift between the document and recorded state.
pub fn drift(config: &UiConfig) -> Result<Json, String> {
    let src = read_source(&config.file)?;
    let doc = compile_source(&src).map_err(|e| format!("compile failed: {e}"))?;
    let recorded = match build_backend(config)? {
        Some(backend) => backend.load_state().map_err(|e| e.to_string())?,
        None => return Ok(json!({ "items": [], "note": "no state file configured" })),
    };
    let report = detect_drift(&doc, &recorded);
    Ok(serde_json::to_value(DriftView::from(&report)).map_err(|e| e.to_string())?)
}

/// Names of providers registered in the default runtime.
pub fn providers() -> Json {
    let runtime = NativeRuntime::new();
    runtime.register(Arc::new(NullProvider::new()));
    let registry = PluginRegistry::new();
    registry.register_runtime("native", Arc::new(runtime));
    json!({ "providers": registry.names() })
}

/// Server capabilities (used by the SPA to know whether `apply` is offered).
pub fn info(config: &UiConfig) -> Json {
    json!({
        "file": config.file.display().to_string(),
        "has_state": config.state.is_some(),
        "allow_apply": config.allow_apply,
        "authenticated": config.token.is_some(),
    })
}

/// Trigger `bastion apply --auto-approve --state <file>` by re-executing the
/// current binary. Only available when `allow_apply` is set.
pub fn apply(config: &UiConfig) -> Result<String, String> {
    if !config.allow_apply {
        return Err("apply is not enabled on this server".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("apply").arg("--auto-approve");
    if let Some(state) = &config.state {
        cmd.arg("--state").arg(state);
    }
    if let Some(var) = &config.passphrase_env {
        cmd.arg("--passphrase-env").arg(var);
    }
    cmd.arg(&config.file);
    let out = cmd.output().map_err(|e| format!("apply failed: {e}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        s.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    Ok(s)
}

#[derive(Serialize)]
struct PlanView {
    to_create: usize,
    to_update: usize,
    to_delete: usize,
    actions: Vec<ActionView>,
}

#[derive(Serialize)]
struct ActionView {
    kind: String,
    id: String,
    type_name: Option<String>,
}

impl From<&bastion_engine::Plan> for PlanView {
    fn from(p: &bastion_engine::Plan) -> Self {
        PlanView {
            to_create: p.to_create,
            to_update: p.to_update,
            to_delete: p.to_delete,
            actions: p
                .actions
                .iter()
                .map(|a| ActionView {
                    kind: a.kind.verb().to_string(),
                    id: a.id.clone(),
                    type_name: a.type_name.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct DriftView {
    items: Vec<DriftItemView>,
}

#[derive(Serialize)]
struct DriftItemView {
    id: String,
    type_name: Option<String>,
    kind: String,
    attr: Option<String>,
    expected: Option<Value>,
    actual: Option<Value>,
}

impl From<&bastion_state::DriftReport> for DriftView {
    fn from(r: &bastion_state::DriftReport) -> Self {
        DriftView {
            items: r
                .items
                .iter()
                .map(|item| {
                    let (kind, attr, expected, actual) = match &item.kind {
                        bastion_state::DriftKind::MissingResource => {
                            ("missing".to_string(), None, None, None)
                        }
                        bastion_state::DriftKind::ExtraResource => {
                            ("extra".to_string(), None, None, None)
                        }
                        bastion_state::DriftKind::AttributeDrift {
                            attr,
                            expected,
                            actual,
                        } => (
                            "changed".to_string(),
                            Some(attr.clone()),
                            expected.clone(),
                            actual.clone(),
                        ),
                    };
                    DriftItemView {
                        id: item.id.clone(),
                        type_name: item.type_name.clone(),
                        kind,
                        attr,
                        expected,
                        actual,
                    }
                })
                .collect(),
        }
    }
}
