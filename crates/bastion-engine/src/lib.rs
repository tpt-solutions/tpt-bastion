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
//!
//! [bastion]: https://bastion.tpt.dev

pub mod error;
pub mod graph;
pub mod plan;

pub use error::EngineError;
pub use graph::DepGraph;
pub use plan::{Action, ActionKind, Plan};

use bastion_ir::{is_compatible, Document, IR_VERSION};

/// Build a [`Plan`] directly from a compiled IR document.
///
/// This is the convenience entry point used by the CLI `plan` subcommand and
/// (eventually) language bindings. It validates IR compatibility and runs the
/// full graph + plan pipeline.
pub fn plan_document(doc: &Document) -> Result<Plan, EngineError> {
    if !is_compatible(doc.ir_version) {
        return Err(EngineError::IncompatibleIr {
            found: doc.ir_version,
            max: IR_VERSION,
        });
    }
    let graph = DepGraph::build(doc)?;
    let order = graph.order()?;
    Plan::from_order(doc, &order)
}
