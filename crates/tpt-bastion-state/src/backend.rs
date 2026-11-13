// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Pluggable state backend interface.
//!
//! [`StateBackend`] is the trait that every state store implements. The crate
//! ships [`MemoryBackend`] (for tests and ephemeral runs) and
//! [`file::FileStateBackend`] (for plaintext or encrypted, lock-aware on-disk
//! storage). External systems — for example TPT Keystone — can implement the
//! same trait to back Bastion state with a remote, managed store without any
//! change to the engine or CLI.

use crate::error::StateError;
use crate::history::StateHistory;
use crate::lock::{make_lock_info, LockGuard, LockInfo};
use crate::state::State;
use std::sync::Mutex;
use std::time::Duration;

/// A storage location for Bastion state.
///
/// Implementors own how bytes are persisted (memory, a local file, an object
/// store, a remote API, …). The engine and CLI only ever talk to this trait, so
/// swapping backends is a one-line construction change.
pub trait StateBackend {
    /// Load and parse the persisted [`State`].
    ///
    /// Encrypted backends transparently decrypt; plaintext backends parse
    /// directly. A missing state yields an empty state (so a first `apply`
    /// starts from nothing) rather than an error.
    fn load_state(&self) -> Result<State, StateError>;

    /// Serialize and persist `state`.
    ///
    /// Encrypted backends transparently encrypt. Implementations should also
    /// append a [`history::StateHistory`] entry so versioning stays consistent
    /// (see [`StateBackend::read_history`]).
    fn save_state(&self, state: &State) -> Result<(), StateError>;

    /// Acquire the cooperative state lock for `holder`.
    ///
    /// Fails with [`StateError::LockHeld`] if another holder owns a live lock.
    /// Returns a [`LockGuard`] that must be presented to [`StateBackend::unlock`]
    /// to release. Dropping the guard does not release the lock; it stays held
    /// until it expires.
    fn lock(&self, holder: &str, ttl: Duration) -> Result<LockGuard, StateError>;

    /// Release a previously-acquired lock.
    fn unlock(&self, guard: &LockGuard) -> Result<(), StateError>;

    /// Read the append-only history of this state's lineage.
    fn read_history(&self) -> Result<StateHistory, StateError>;

    /// Convenience: save `state` and return the updated history.
    fn save_and_history(&self, state: &State) -> Result<StateHistory, StateError> {
        self.save_state(state)?;
        self.read_history()
    }
}

/// An in-memory [`StateBackend`] used for tests, ephemeral runs, and as the
/// reference implementation of the backend trait. State, history, and the lock
/// all live behind a mutex so the backend is `Sync`.
pub struct MemoryBackend {
    inner: Mutex<MemInner>,
}

struct MemInner {
    state: Option<State>,
    history: StateHistory,
    lock: Option<LockInfo>,
}

impl MemoryBackend {
    /// A fresh, empty in-memory backend.
    pub fn new() -> Self {
        MemoryBackend {
            inner: Mutex::new(MemInner {
                state: None,
                history: StateHistory::new(),
                lock: None,
            }),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StateBackend for MemoryBackend {
    fn load_state(&self) -> Result<State, StateError> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| StateError::Backend("memory lock poisoned".into()))?;
        Ok(guard.state.clone().unwrap_or_else(State::new))
    }

    fn save_state(&self, state: &State) -> Result<(), StateError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| StateError::Backend("memory lock poisoned".into()))?;
        let prev = guard.state.clone();
        guard.state = Some(state.clone());
        guard.history.record(state, prev.as_ref())?;
        Ok(())
    }

    fn lock(&self, holder: &str, ttl: Duration) -> Result<LockGuard, StateError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| StateError::Backend("memory lock poisoned".into()))?;
        if let Some(current) = &guard.lock {
            if !crate::lock::is_expired(current) {
                return Err(StateError::LockHeld {
                    holder: current.holder.clone(),
                    acquired_at: current.acquired_at.clone(),
                    expires_at: current.expires_at.clone(),
                });
            }
        }
        let info = make_lock_info(holder, ttl);
        let nonce = info.nonce.clone();
        guard.lock = Some(info);
        Ok(LockGuard {
            backend_key: "memory".into(),
            holder: holder.to_string(),
            nonce,
        })
    }

    fn unlock(&self, guard: &LockGuard) -> Result<(), StateError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| StateError::Backend("memory lock poisoned".into()))?;
        match g.lock.take() {
            None => Ok(()),
            Some(current) => {
                if current.nonce != guard.nonce || current.holder != guard.holder {
                    // Put it back; we didn't own it.
                    g.lock = Some(current.clone());
                    return Err(StateError::LockMismatch(current.holder));
                }
                Ok(())
            }
        }
    }

    fn read_history(&self) -> Result<StateHistory, StateError> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| StateError::Backend("memory lock poisoned".into()))?;
        Ok(guard.history.clone())
    }
}
