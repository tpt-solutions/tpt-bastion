// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Execution observability: the event stream emitted during plan/apply.
//!
//! The engine is deliberately silent by default. Callers (the CLI, language
//! bindings, test harnesses) subscribe to [`Event`]s by implementing
//! [`EventHandler`] and passing a handler to the [`crate::executor::Executor`].
//! Two ready-made handlers ship with the crate: [`PrintingHandler`] (human
//! readable, used by the CLI) and [`NoopHandler`] (drops everything, used in
//! tests and embedded contexts).

use crate::provider::AppliedResource;

/// A point-in-time occurrence during a plan or apply run.
///
/// Every variant carries just enough context to render a log line or feed an
/// external telemetry sink without coupling the engine to any one backend.
#[derive(Debug, Clone)]
pub enum Event {
    /// An apply run is starting. `plan` is the rendered plan summary.
    ApplyStart {
        /// Number of resources to create.
        to_create: usize,
        /// Number of resources to update.
        to_update: usize,
        /// Number of resources to delete.
        to_delete: usize,
    },

    /// A dependency level is about to be worked on. All ids in the batch may
    /// run in parallel because none depends on another within the batch.
    BatchStart {
        /// Zero-based dependency level.
        level: usize,
        /// Resource ids in this batch.
        ids: Vec<String>,
    },

    /// A single resource mutation has begun.
    ResourceStart {
        /// Resource id being acted on.
        id: String,
    },

    /// A resource was successfully created/applied by the provider.
    ResourceApplied {
        /// The resolved, post-apply resource state.
        resource: AppliedResource,
        /// Dependency level it was applied at.
        level: usize,
    },

    /// A provider failed to apply a resource.
    ResourceFailed {
        /// Resource id that failed.
        id: String,
        /// Provider-reported error message.
        error: String,
    },

    /// A failure triggered a rollback; this is emitted before any destroys.
    RollbackStart {
        /// How many applied resources will be torn down.
        applied_count: usize,
    },

    /// A previously-applied resource was destroyed during rollback.
    ResourceRolledBack {
        /// Resource id that was destroyed.
        id: String,
    },

    /// A resource present in prior state but absent from the document was
    /// intentionally destroyed as part of a successful apply.
    ResourceDeleted {
        /// Resource id that was destroyed.
        id: String,
    },

    /// Rollback finished; the infrastructure is back to its prior state.
    RollbackComplete,

    /// The whole apply run finished successfully.
    ApplyComplete {
        /// Resources created this run.
        created: usize,
        /// Resources updated this run.
        updated: usize,
        /// Resources deleted this run.
        deleted: usize,
    },
}

/// Receives [`Event`]s emitted by the executor.
///
/// Implementors must be cheap to share across the parallel scheduler, hence
/// `Sync`. A handler is never told *when* a run ends other than via the
/// `ApplyComplete` / `RollbackComplete` events, so it should not assume
/// event counts in advance.
pub trait EventHandler: Send + Sync {
    /// Handle a single emitted event.
    fn handle(&self, event: &Event);
}

/// A handler that prints human-readable lines to stdout/stderr.
///
/// Apply-progress events go to stdout; rollback/destructive events go to
/// stderr so they remain visible even when stdout is piped.
pub struct PrintingHandler {
    /// When false, only rollback/deletion events are emitted (quiet mode).
    pub verbose: bool,
}

impl Default for PrintingHandler {
    fn default() -> Self {
        PrintingHandler { verbose: true }
    }
}

impl EventHandler for PrintingHandler {
    fn handle(&self, event: &Event) {
        match event {
            Event::ApplyStart {
                to_create,
                to_update,
                to_delete,
            } => {
                if self.verbose {
                    println!(
                        "bastion: applying plan (+{}, ~{}, -{})",
                        to_create, to_update, to_delete
                    );
                }
            }
            Event::BatchStart { level, ids } => {
                if self.verbose {
                    println!("  level {}: {} resources", level, ids.len());
                }
            }
            Event::ResourceStart { id } => {
                if self.verbose {
                    println!("  -> {id}");
                }
            }
            Event::ResourceApplied { resource, .. } => {
                if self.verbose {
                    println!("  ok  {} ({})", resource.id, resource.type_name);
                }
            }
            Event::ResourceFailed { id, error } => {
                eprintln!("  FAIL {id}: {error}");
            }
            Event::RollbackStart { applied_count } => {
                eprintln!("bastion: rolling back {applied_count} applied resource(s)");
            }
            Event::ResourceRolledBack { id } => {
                eprintln!("  rolled back {id}");
            }
            Event::ResourceDeleted { id } => {
                if self.verbose {
                    println!("  - {id}");
                }
            }
            Event::RollbackComplete => {
                eprintln!("bastion: rollback complete");
            }
            Event::ApplyComplete {
                created,
                updated,
                deleted,
            } => {
                if self.verbose {
                    println!(
                        "bastion: apply complete (+{}, ~{}, -{})",
                        created, updated, deleted
                    );
                }
            }
        }
    }
}

/// A handler that discards every event. Useful for tests and embedded use.
pub struct NoopHandler;

impl EventHandler for NoopHandler {
    fn handle(&self, _event: &Event) {}
}
