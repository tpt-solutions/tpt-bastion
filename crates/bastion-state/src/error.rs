// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Error types for the Bastion native state layer.

use thiserror::Error;

/// Errors produced by state backends, encryption, locking, history, and drift.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StateError {
    /// A state file could not be read from disk.
    #[error("state io error: {0}")]
    Io(String),

    /// A state blob could not be serialized or parsed.
    #[error("state serialization error: {0}")]
    Encode(String),

    /// A passphrase was required for an encrypted state but none was supplied.
    #[error("state is encrypted but no passphrase was provided")]
    MissingPassphrase,

    /// A passphrase was supplied but the state is not encrypted.
    #[error("passphrase supplied for a state that is not encrypted")]
    UnexpectedPassphrase,

    /// The encrypted envelope was malformed or its magic/version was unknown.
    #[error("unsupported or corrupted state envelope: {0}")]
    Envelope(String),

    /// Decryption failed (wrong passphrase or tampered ciphertext).
    #[error("state decryption failed (wrong passphrase or corrupted data)")]
    Decrypt,

    /// Key derivation from the passphrase failed.
    #[error("key derivation failed: {0}")]
    KeyDerivation(String),

    /// The state file format version is incompatible with this build.
    #[error("state format version {found} is incompatible (max supported {max})")]
    IncompatibleVersion { found: u32, max: u32 },

    /// A lock could not be acquired because another holder owns it.
    #[error("state is locked by `{holder}` (acquired at {acquired_at}, expires {expires_at})")]
    LockHeld {
        /// Identity of the current lock holder.
        holder: String,
        /// RFC3339 acquisition timestamp.
        acquired_at: String,
        /// RFC3339 expiry timestamp.
        expires_at: String,
    },

    /// An unlock was attempted with a guard that does not match the lock file.
    #[error("lock guard does not match the current lock (held by `{0}`)")]
    LockMismatch(String),

    /// A backend operation was attempted without first holding the lock.
    #[error("operation requires the state lock held by `{0}`")]
    LockRequired(String),

    /// The underlying backend rejected an operation.
    #[error("backend error: {0}")]
    Backend(String),
}
