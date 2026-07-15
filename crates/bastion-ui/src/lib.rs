// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # bastion-ui
//!
//! A self-contained, dependency-light local web dashboard for [TPT Bastion].
//!
//! `bastion ui` starts a tiny HTTP server (no external framework) bound to
//! `127.0.0.1` that serves a single embedded HTML/JS single-page app and a
//! small JSON API. The API reuses the core crates to compile, plan, inspect
//! state, show history, and detect drift for a `.bcl` document — giving you a
//! live, visual view of your infrastructure without leaving the terminal
//! workflow.
//!
//! Security: the server binds loopback only and supports an optional bearer
//! token (`--token`). It never reads or writes anything outside the configured
//! `.bcl`/state files, and the `apply` action is gated behind an explicit flag.
//!
//! [bastion]: https://bastion.tpt.dev

pub mod api;
pub mod assets;
pub mod server;

use std::path::PathBuf;

/// Configuration for the dashboard server.
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// The `.bcl` document the dashboard operates on.
    pub file: PathBuf,
    /// Optional state file for plan diffs, state inspection, and drift.
    pub state: Option<PathBuf>,
    /// TCP port to bind on `127.0.0.1` (default `7474`).
    pub port: u16,
    /// Optional bearer token required on every `/api/*` request.
    pub token: Option<String>,
    /// Optional environment variable holding the state encryption passphrase.
    pub passphrase_env: Option<String>,
    /// Whether to expose the mutating `POST /api/apply` endpoint.
    pub allow_apply: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            file: PathBuf::from("main.bcl"),
            state: None,
            port: 7474,
            token: None,
            passphrase_env: None,
            allow_apply: false,
        }
    }
}

/// Start the dashboard server and run until the process is terminated.
///
/// Binds `127.0.0.1:<port>` and serves the embedded SPA plus the JSON API.
pub fn serve(config: UiConfig) -> anyhow::Result<()> {
    server::run(config)
}
