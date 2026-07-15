// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Append-only version history for a state lineage.
//!
//! Every successful [`StateBackend::save_state`] appends a [`HistoryEntry`] so
//! operators can answer "what changed, when, and what did state look like at
//! revision N". The history is keyed by [`State::lineage`] so forks of the same
//! stack share a tail and divergent stacks stay separate.

use crate::error::StateError;
use crate::state::State;
use bastion_engine::provider::AppliedResource;
use serde::{Deserialize, Serialize};

/// A compact summary of the resource-account changes in a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistorySummary {
    /// Resources newly present in this revision.
    pub created: usize,
    /// Resources present before and after but with changed attributes.
    pub updated: usize,
    /// Resources removed in this revision.
    pub deleted: usize,
    /// Total resources tracked after this revision.
    pub total: usize,
}

/// A single revision in a state's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Monotonic index within the lineage (starts at 1).
    pub index: u64,
    /// RFC3339 timestamp the revision was recorded.
    pub timestamp: String,
    /// Lineage this revision belongs to.
    pub lineage: String,
    /// Content fingerprint ([`State::fingerprint`]) of the revision's state.
    pub state_hash: String,
    /// Resource-account change summary.
    pub summary: HistorySummary,
}

/// The ordered, append-only history of a single lineage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateHistory {
    /// Revision entries in chronological order.
    pub entries: Vec<HistoryEntry>,
}

impl StateHistory {
    /// An empty history.
    pub fn new() -> Self {
        StateHistory {
            entries: Vec::new(),
        }
    }

    /// Number of recorded revisions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if no revisions have been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record `state` as a new revision. `prev` (the revision that will be
    /// replaced) is used to derive an accurate change summary; pass `None` for
    /// the first revision. Returns the newly appended entry.
    ///
    /// Revisions whose content fingerprint is identical to the latest entry are
    /// not recorded again (idempotent writes).
    pub fn record(
        &mut self,
        state: &State,
        prev: Option<&State>,
    ) -> Result<&HistoryEntry, StateError> {
        let fingerprint = state.fingerprint()?;
        if let Some(latest) = self.entries.last() {
            if latest.state_hash == fingerprint && latest.lineage == state.lineage {
                return Ok(self.entries.last().unwrap());
            }
        }
        let summary = match prev {
            Some(p) => diff_summary(&p.resources, &state.resources),
            None => HistorySummary {
                created: state.resources.len(),
                updated: 0,
                deleted: 0,
                total: state.resources.len(),
            },
        };
        let index = self.entries.len() as u64 + 1;
        let entry = HistoryEntry {
            index,
            timestamp: state.updated_at.clone(),
            lineage: state.lineage.clone(),
            state_hash: fingerprint,
            summary,
        };
        self.entries.push(entry);
        Ok(self.entries.last().unwrap())
    }

    /// The most recent revision, if any.
    pub fn latest(&self) -> Option<&HistoryEntry> {
        self.entries.last()
    }

    /// Serialize the history as pretty JSON.
    pub fn to_json(&self) -> Result<String, StateError> {
        serde_json::to_string_pretty(self).map_err(|e| StateError::Encode(e.to_string()))
    }

    /// Parse history from JSON produced by [`StateHistory::to_json`].
    pub fn from_json(text: &str) -> Result<StateHistory, StateError> {
        serde_json::from_str(text).map_err(|e| StateError::Encode(e.to_string()))
    }
}

/// Compute a [`HistorySummary`] comparing `prev` and `next` resource lists by id.
fn diff_summary(prev: &[AppliedResource], next: &[AppliedResource]) -> HistorySummary {
    let prev_ids: std::collections::BTreeSet<&str> = prev.iter().map(|r| r.id.as_str()).collect();
    let next_ids: std::collections::BTreeSet<&str> = next.iter().map(|r| r.id.as_str()).collect();

    let created = next_ids.difference(&prev_ids).count();
    let deleted = prev_ids.difference(&next_ids).count();
    // Present in both: counted as "updated" for the purpose of a change summary.
    let updated = prev_ids.intersection(&next_ids).count();

    HistorySummary {
        created,
        updated,
        deleted,
        total: next.len(),
    }
}
