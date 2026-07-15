// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cooperative state locking and concurrency control.
//!
//! Bastion uses a *cooperative* file lock: a small JSON file next to the state
//! recording who holds the lock, when it was acquired, and when it expires.
//! Locks are not enforced by the OS — every writer cooperates by checking the
//! lock before mutating state — which is the same model Terraform uses and is
//! sufficient for a CLI tool where a single process owns an apply at a time.

use crate::error::StateError;
use crate::state::now_rfc3339;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Metadata written into a `.lock` file while a holder owns the state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockInfo {
    /// Identity of the process/session holding the lock (e.g. hostname+pid).
    pub holder: String,
    /// RFC3339 acquisition timestamp.
    pub acquired_at: String,
    /// RFC3339 expiry timestamp.
    pub expires_at: String,
    /// Random nonce validating that an unlock comes from the acquiring holder.
    pub nonce: String,
}

/// A live lock held by a caller. Present this to [`crate::StateBackend::unlock`]
/// to release. Dropping it does **not** auto-release (explicit release keeps
/// the concurrency semantics predictable).
#[derive(Debug, Clone)]
pub struct LockGuard {
    /// The backend identity this guard belongs to (file path or in-memory id).
    pub backend_key: String,
    /// Holder identity recorded at acquisition.
    pub holder: String,
    /// Nonce that must match the on-disk lock to release it.
    pub nonce: String,
}

/// Parse a lock file if it exists and is valid JSON.
pub fn read_lock_file(lock_path: &Path) -> Result<Option<LockInfo>, StateError> {
    if !lock_path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(lock_path).map_err(|e| StateError::Io(e.to_string()))?;
    let info: LockInfo = serde_json::from_str(&text).map_err(|e| StateError::Io(e.to_string()))?;
    Ok(Some(info))
}

/// Acquire the lock file at `lock_path` for `holder`, failing if a live lock is
/// held by someone else. Stale locks (past `expires_at`) are taken over.
pub fn acquire_lock_file(
    lock_path: &Path,
    holder: &str,
    ttl: Duration,
) -> Result<LockGuard, StateError> {
    if let Some(existing) = read_lock_file(lock_path)? {
        if !is_expired(&existing) {
            return Err(StateError::LockHeld {
                holder: existing.holder,
                acquired_at: existing.acquired_at,
                expires_at: existing.expires_at,
            });
        }
    }

    let info = make_lock_info(holder, ttl);
    let nonce = info.nonce.clone();
    let text =
        serde_json::to_string_pretty(&info).map_err(|e| StateError::Encode(e.to_string()))?;
    // Best-effort exclusive create; if it already exists and is a stale lock we
    // overwrite below.
    std::fs::write(lock_path, text).map_err(|e| StateError::Io(e.to_string()))?;
    Ok(LockGuard {
        backend_key: lock_path.to_string_lossy().into_owned(),
        holder: holder.to_string(),
        nonce,
    })
}

/// Release a lock file, validating that `guard` matches the current holder.
pub fn release_lock_file(lock_path: &Path, guard: &LockGuard) -> Result<(), StateError> {
    match read_lock_file(lock_path)? {
        None => Ok(()),
        Some(current) => {
            if current.nonce != guard.nonce || current.holder != guard.holder {
                return Err(StateError::LockMismatch(current.holder));
            }
            std::fs::remove_file(lock_path).map_err(|e| StateError::Io(e.to_string()))?;
            Ok(())
        }
    }
}

/// Returns `true` if the lock's `expires_at` is in the past.
pub fn is_expired(info: &LockInfo) -> bool {
    // Compare lexically-safe by re-parsing the RFC3339-ish strings into epochs
    // is overkill; instead compare the fixed-width strings (ISO UTC sorts
    // correctly lexically).
    let now = now_rfc3339();
    now.as_str() > info.expires_at.as_str()
}

/// Build a fresh [`LockInfo`] for `holder` valid for `ttl`, used by both the
/// file backend and the in-memory backend.
pub fn make_lock_info(holder: &str, ttl: Duration) -> LockInfo {
    let acquired_at = now_rfc3339();
    let expires = std::time::SystemTime::now() + ttl;
    let (y, mo, d, h, mi, s) = crate::state::break_down_utc_export(
        expires
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );
    LockInfo {
        holder: holder.to_string(),
        acquired_at,
        expires_at: format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z"),
        nonce: random_nonce(),
    }
}

fn random_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let r = rand::random::<u64>().wrapping_add(nanos as u64);
    format!("{r:016x}")
}
