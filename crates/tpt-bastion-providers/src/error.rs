// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Errors for the provider plugin system.

use thiserror::Error;

/// An error raised by a provider plugin while performing a lifecycle op.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderError {
    /// The provider does not implement the requested lifecycle op.
    #[error("provider `{provider}` does not support `{op}`")]
    UnsupportedOp { provider: String, op: String },

    /// The provider rejected the request (e.g. a missing required attribute).
    #[error("provider `{provider}`: {message}")]
    Rejected { provider: String, message: String },

    /// The provider panicked / returned an unexpected internal failure.
    #[error("provider `{provider}` internal error: {message}")]
    Internal { provider: String, message: String },

    /// A requested resource type is unknown to the provider.
    #[error("provider `{provider}` has no resource type `{type_name}`")]
    UnknownResourceType { provider: String, type_name: String },
}

/// An error raised by the host while locating or driving a plugin runtime.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeError {
    /// No runtime could resolve a plugin by this name.
    #[error("no plugin named `{0}` is registered")]
    NotFound(String),

    /// The plugin process could not be spawned / its IPC channel broke.
    #[error("plugin process error for `{name}`: {message}")]
    Process { name: String, message: String },

    /// The plugin spoke an ABI message the host could not decode.
    #[error("plugin ABI decode error: {0}")]
    Abi(String),
}
