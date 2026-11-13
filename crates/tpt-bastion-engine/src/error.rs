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
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("unknown reference target `{0}` (no such resource or module)")]
    UnknownReference(String),

    /// The dependency graph contains a cycle; the listed ids form the cycle.
    #[error("dependency cycle detected: {}", .path.join(" -> "))]
    Cycle {
        /// The ids participating in the cycle, in order.
        path: Vec<String>,
    },

    /// A reference could not be resolved against already-applied resources at
    /// apply time (e.g. the target has no such attribute).
    #[error("could not resolve reference to `{base}`: {detail}")]
    ReferenceResolution {
        /// The base identifier the reference pointed at.
        base: String,
        /// Human-readable reason the resolution failed.
        detail: String,
    },

    /// A provider reported a failure while applying or destroying a resource.
    #[error("provider error: {0}")]
    Provider(String),

    /// A state file could not be read or written.
    #[error("state io error: {0}")]
    StateIo(String),

    /// A state file could not be serialized or parsed.
    #[error("state serialization error: {0}")]
    StateEncode(String),
}
