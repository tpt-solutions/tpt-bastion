// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal execution state.
//!
//! This is the seed of the native state backend described in Phase 4. It is
//! intentionally small and plaintext here; encryption, versioned history,
//! locking, and drift detection are layered on in that phase.

use crate::error::EngineError;
use crate::provider::AppliedResource;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The state file format version. Bumped only on breaking shape changes.
pub const STATE_VERSION: u32 = 1;

/// A snapshot of applied infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// State format version, matched against [`STATE_VERSION`] on load.
    pub version: u32,
    /// Resources tracked by this state.
    pub resources: Vec<AppliedResource>,
}

impl State {
    /// An empty state with the current format version.
    pub fn new() -> Self {
        State {
            version: STATE_VERSION,
            resources: Vec::new(),
        }
    }

    /// Build a state from a list of applied resources.
    pub fn from_applied(resources: Vec<AppliedResource>) -> Self {
        State {
            version: STATE_VERSION,
            resources,
        }
    }

    /// Persist the state as pretty JSON.
    pub fn save(&self, path: &Path) -> Result<(), EngineError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| EngineError::StateEncode(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| EngineError::StateIo(e.to_string()))?;
        Ok(())
    }

    /// Load and parse a state file written by [`State::save`].
    pub fn load(path: &Path) -> Result<State, EngineError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| EngineError::StateIo(e.to_string()))?;
        let state: State =
            serde_json::from_str(&text).map_err(|e| EngineError::StateEncode(e.to_string()))?;
        Ok(state)
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
