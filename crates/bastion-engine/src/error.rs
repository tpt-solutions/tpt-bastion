// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Error types for the Bastion execution engine.

use thiserror::Error;

/// Errors produced while building dependency graphs or plans.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    /// The IR version carried by a document is newer than this engine accepts.
    #[error("IR version {found} is incompatible (max supported {max})")]
    IncompatibleIr {
        /// Version declared by the document.
        found: u32,
        /// Highest version this engine can consume.
        max: u32,
    },

    /// A reference points at a base object (resource/module/variable) that does
    /// not exist in the document.
    #[error("unknown reference target `{0}` (no such resource or module)")]
    UnknownReference(String),

    /// A dependency cycle was detected while ordering the graph.
    #[error("dependency cycle detected: {0:?}")]
    Cycle {
        /// The cyclic path of node ids, from the repeated node back to itself.
        path: Vec<String>,
    },
}
