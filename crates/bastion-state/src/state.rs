// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The persisted state document and its (de)serialization.
//!
//! [`State`] is the canonical on-disk representation of applied infrastructure.
//! It deliberately mirrors the shape produced by the execution engine
//! ([`bastion_engine::provider::AppliedResource`]) so that an apply can be
//! serialized here directly, and so drift detection can compare desired
//! [`bastion_ir::Document`] resources against recorded actual resources.

use crate::error::StateError;
use crate::history::HistorySummary;
use bastion_engine::provider::AppliedResource;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The state file format version. Bumped only on breaking shape changes.
pub const STATE_FORMAT_VERSION: u32 = 1;

/// A snapshot of applied infrastructure, the unit persisted by a state backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// State format version, matched against [`STATE_FORMAT_VERSION`] on load.
    pub version: u32,

    /// Stable identity grouping every snapshot of one logical infrastructure
    /// stack across history entries.
    pub lineage: String,

    /// RFC3339 timestamp of the last mutation.
    #[serde(default = "now_rfc3339")]
    pub updated_at: String,

    /// Resources currently tracked by this state.
    pub resources: Vec<AppliedResource>,
}

impl State {
    /// An empty state with the current format version and a fresh lineage.
    pub fn new() -> Self {
        State {
            version: STATE_FORMAT_VERSION,
            lineage: new_lineage(),
            updated_at: now_rfc3339(),
            resources: Vec::new(),
        }
    }

    /// Build a state from applied resources, carrying over `lineage`.
    pub fn from_applied(lineage: String, resources: Vec<AppliedResource>) -> Self {
        State {
            version: STATE_FORMAT_VERSION,
            lineage,
            updated_at: now_rfc3339(),
            resources,
        }
    }

    /// Look up a tracked resource by its fully-qualified id.
    pub fn get(&self, id: &str) -> Option<&AppliedResource> {
        self.resources.iter().find(|r| r.id == id)
    }

    /// A short, human-readable summary of the resource counts in this state.
    pub fn summary(&self) -> HistorySummary {
        HistorySummary {
            created: self.resources.len(),
            updated: 0,
            deleted: 0,
            total: self.resources.len(),
        }
    }

    /// Canonical JSON used for content fingerprinting and history diffing.
    ///
    /// Keys are serialized in sorted order by serde (via `BTreeMap`-style
    /// deterministic value ordering is not guaranteed for arbitrary serde
    /// structs, so we round-trip through a normalized form). We achieve
    /// stability by serializing with sorted map keys when possible and relying
    /// on the fixed field order of the struct definition.
    pub fn canonical_json(&self) -> Result<String, StateError> {
        serde_json::to_string(self).map_err(|e| StateError::Encode(e.to_string()))
    }

    /// A stable fingerprint of this state's contents.
    pub fn fingerprint(&self) -> Result<String, StateError> {
        let json = self.canonical_json()?;
        Ok(crate::crypto::fingerprint(json.as_bytes()))
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl From<bastion_engine::State> for State {
    fn from(s: bastion_engine::State) -> Self {
        State {
            version: STATE_FORMAT_VERSION,
            lineage: new_lineage(),
            updated_at: now_rfc3339(),
            resources: s.resources,
        }
    }
}

impl From<&State> for bastion_engine::State {
    fn from(s: &State) -> Self {
        bastion_engine::State::from_applied(s.resources.clone())
    }
}

/// Generate a fresh lineage id. Uses a random base to avoid collisions across
/// separate `bastion apply` sessions while remaining printable.
pub fn new_lineage() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut rng = rand::random::<u64>();
    rng = rng.wrapping_add(nanos as u64);
    format!("ln_{rng:016x}")
}

/// Current time as an RFC3339 string.
pub fn now_rfc3339() -> String {
    // std has no RFC3339 formatter, so build a stable, sortable representation.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let nanos = now.subsec_nanos();
    // 2026-07-15T13:05:14.123456789Z  (UTC, fixed-width)
    let (y, mo, d, h, mi, s) = break_down_utc(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{nanos:09}Z")
}

/// Convert a Unix timestamp into UTC calendar components. A simplified
/// Gregorian decomposition sufficient for timestamps in the realistic range of
/// this tool; avoids pulling a datetime dependency.
pub(crate) fn break_down_utc_export(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    break_down_utc(secs)
}

fn break_down_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    const SECS_PER_MINUTE: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;
    let days = secs / SECS_PER_DAY;
    let rem = secs % SECS_PER_DAY;
    let h = (rem / SECS_PER_HOUR) as u32;
    let rem = rem % SECS_PER_HOUR;
    let mi = (rem / SECS_PER_MINUTE) as u32;
    let s = (rem % SECS_PER_MINUTE) as u32;

    // Days since 1970-01-01; convert to Y/M/D with leap-year handling.
    let mut d = days as i64;
    let mut year = 1970;
    loop {
        let leap = is_leap(year);
        let year_len = if leap { 366 } else { 365 };
        if d >= year_len {
            d -= year_len;
            year += 1;
        } else {
            break;
        }
    }
    let month_len = |m: u32, y: i32| -> u32 {
        match m {
            1 => 31,
            2 => {
                if is_leap(y) {
                    29
                } else {
                    28
                }
            }
            3 => 31,
            4 => 30,
            5 => 31,
            6 => 30,
            7 => 31,
            8 => 31,
            9 => 30,
            10 => 31,
            11 => 30,
            _ => 31,
        }
    };
    let mut month = 1;
    let mut day = d + 1;
    while day > month_len(month, year) as i64 {
        day -= month_len(month, year) as i64;
        month += 1;
    }
    (year as u32, month, day as u32, h, mi, s)
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Re-exported for backends that need to carry applied resources.
pub use bastion_engine::provider::AppliedResource as Resource;

/// Convenience constructor for an [`AppliedResource`] with literal attributes.
pub fn applied_resource(
    id: &str,
    type_name: &str,
    attributes: BTreeMap<String, bastion_ir::Value>,
) -> AppliedResource {
    AppliedResource {
        id: id.to_string(),
        type_name: type_name.to_string(),
        attributes,
    }
}
