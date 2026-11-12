// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # tpt-bastion-state
//!
//! Native state management for [TPT Bastion][bastion]. This crate is the
//! production realization of the `state.rs` seed that shipped with
//! [`tpt_bastion_engine`] (Phase 3) and implements everything tracked under
//! **Phase 4** of the project roadmap:
//!
//! - **Encrypted state file format** — at-rest encryption with AES-256-GCM,
//!   keys derived from a passphrase via Argon2id (see [`crypto`]).
//! - **Versioning / history** — every write records an append-only
//!   [`history::StateHistory`] entry with a stable lineage id and content hash
//!   ([`history`]).
//! - **Locking / concurrency control** — cooperative file locks with a holder
//!   identity and expiry, exposed through [`backend::StateBackend::lock`]
//!   ([`lock`]).
//! - **Drift detection** — compare the desired IR document against the recorded
//!   actual state to surface added/removed/changed resources
//!   ([`drift::detect_drift`]).
//! - **Reconciliation** — turn a [`drift::DriftReport`] into a concrete
//!   [`tpt_bastion_engine::Plan`] that the executor can apply to bring reality back
//!   in line with the document ([`reconcile::reconcile`]).
//! - **Pluggable backend interface** — [`backend::StateBackend`] is the trait
//!   that external state stores (e.g. TPT Keystone) implement. The crate ships
//!   an in-memory backend for tests and a file backend that supports both
//!   plaintext and encrypted, lock-aware storage ([`file::FileStateBackend`]).
//!
//! The crate depends only on the IR and engine crates, so it can be published
//! and consumed independently.
//!
//! [bastion]: https://bastion.tpt.dev

pub mod backend;
pub mod crypto;
pub mod drift;
pub mod error;
pub mod file;
pub mod history;
pub mod lock;
pub mod reconcile;
pub mod state;

pub use backend::{MemoryBackend, StateBackend};
pub use error::StateError;
pub use file::FileStateBackend;
pub use history::{HistoryEntry, HistorySummary, StateHistory};
pub use lock::{LockGuard, LockInfo};
pub use state::State;
