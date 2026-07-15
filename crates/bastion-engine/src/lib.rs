// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # bastion-engine
//!
//! The execution core for [TPT Bastion][bastion]: it turns a compiled
//! [`bastion_ir::Document`] into a dependency graph and a concrete plan that
//! the apply stage (and future providers) will execute.
//!
//! The pipeline is intentionally decoupled from the DSL:
//!
//! 1. [`graph::DepGraph::build`] reads references in the IR to construct a
//!    dependency graph between resources (and outputs).
//! 2. [`graph::DepGraph::order`] topologically sorts the graph so that a
//!    resource is only acted on after everything it references already exists.
//! 3. [`plan::Plan::from_document`] walks that order and emits the sequence of
//!    [`plan::Action`]s the engine will perform.
//! 4. [`executor::Executor`] schedules those actions (in parallel where
//!    dependencies allow) and runs them through a [`provider::Provider`],
//!    rolling back on failure and emitting [`observability::Event`]s.
//!
//! [bastion]: https://bastion.tpt.dev

pub mod error;
pub mod executor;
pub mod graph;
pub mod observability;
pub mod plan;
pub mod provider;
pub mod state;

pub use error::EngineError;
pub use executor::Executor;
pub use graph::DepGraph;
pub use observability::{Event, EventHandler, NoopHandler, PrintingHandler};
pub use plan::{Action, ActionKind, Plan};
pub use provider::{AppliedResource, MockProvider, Provider};
pub use state::State;

use bastion_ir::{is_compatible, Document, IR_VERSION};

/// Build a [`Plan`] directly from a compiled IR document, diffing against an
/// empty prior state (every resource plans as a create).
///
/// This is the convenience entry point used by the CLI `plan` subcommand and
/// (eventually) language bindings. It validates IR compatibility and runs the
/// full graph + plan pipeline.
pub fn plan_document(doc: &Document) -> Result<Plan, EngineError> {
    plan_document_with_state(doc, &State::new())
}

/// Build a [`Plan`] from a document, diffing against `prior` state so that
/// existing resources plan as updates and removed ones as deletes.
pub fn plan_document_with_state(doc: &Document, prior: &State) -> Result<Plan, EngineError> {
    if !is_compatible(doc.ir_version) {
        return Err(EngineError::IncompatibleIr {
            found: doc.ir_version,
            max: IR_VERSION,
        });
    }
    let graph = DepGraph::build(doc)?;
    let order = graph.order()?;
    Plan::from_order(doc, &order, prior)
}
