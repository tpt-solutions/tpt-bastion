// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Real-time drift detection.
//!
//! Drift is the gap between what the [`tpt_bastion_ir::Document`] *declares* and what
//! the recorded [`State`] *actually* holds. This module produces a
//! [`DriftReport`] by diffing the desired resources against the applied
//! resources. Reference-valued attributes in the document are skipped, because
//! their concrete value is only known after a provider fills it in at apply
//! time — comparing a `Value::Reference` against a literal would produce false
//! positives.

use crate::state::State;
use std::collections::BTreeMap;
use tpt_bastion_ir::{Document, Value};

/// The category of a detected drift.
#[derive(Debug, Clone, PartialEq)]
pub enum DriftKind {
    /// A desired resource is absent from the recorded state.
    MissingResource,
    /// A recorded resource is not present in the document (orphaned).
    ExtraResource,
    /// A specific attribute differs between desired and recorded state.
    AttributeDrift {
        /// The attribute name.
        attr: String,
        /// The value declared in the document (`None` if the attribute was
        /// removed from the document).
        expected: Option<Value>,
        /// The value recorded in state (`None` if the attribute is absent from
        /// the recorded resource).
        actual: Option<Value>,
    },
}

/// A single drifted element.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftItem {
    /// The fully-qualified resource id, or empty for document-level drift.
    pub id: String,
    /// The resource type, when known.
    pub type_name: Option<String>,
    /// What kind of drift this is.
    pub kind: DriftKind,
}

/// The result of comparing a desired document against recorded state.
#[derive(Debug, Clone, Default)]
pub struct DriftReport {
    /// All drifted items.
    pub items: Vec<DriftItem>,
}

impl DriftReport {
    /// `true` if no drift was detected.
    pub fn is_clean(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of drifted items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` if there are no drifted items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Render the report as a human-readable summary.
    pub fn render(&self) -> String {
        if self.is_clean() {
            return "Drift: no drift detected — infrastructure matches the document.\n".to_string();
        }
        let mut out = String::new();
        out.push_str(&format!(
            "Drift: {} item(s) detected — infrastructure differs from the document.\n",
            self.items.len()
        ));
        out.push('\n');
        for (i, item) in self.items.iter().enumerate() {
            let ty = item.type_name.as_deref().unwrap_or("?");
            match &item.kind {
                DriftKind::MissingResource => {
                    out.push_str(&format!(
                        "  {:>2}. MISSING   {} ({}) — declared but not applied\n",
                        i + 1,
                        item.id,
                        ty
                    ));
                }
                DriftKind::ExtraResource => {
                    out.push_str(&format!(
                        "  {:>2}. EXTRA     {} ({}) — applied but not declared\n",
                        i + 1,
                        item.id,
                        ty
                    ));
                }
                DriftKind::AttributeDrift {
                    attr,
                    expected,
                    actual,
                } => {
                    let exp = value_label(expected);
                    let act = value_label(actual);
                    out.push_str(&format!(
                        "  {:>2}. CHANGED   {}.{} ({}): expected {}, found {}\n",
                        i + 1,
                        item.id,
                        attr,
                        ty,
                        exp,
                        act
                    ));
                }
            }
        }
        out
    }
}

fn value_label(v: &Option<Value>) -> String {
    match v {
        None => "<absent>".to_string(),
        Some(Value::String(s)) => format!("\"{s}\""),
        Some(Value::Number(n)) => format!("{n}"),
        Some(Value::Bool(b)) => format!("{b}"),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Reference { base, path }) => {
            format!("{base}.{}", path.join("."))
        }
        Some(Value::List(_)) => "<list>".to_string(),
        Some(Value::Map(_)) => "<map>".to_string(),
    }
}

/// Detect drift between a desired [`Document`] and the recorded [`State`].
///
/// Reference-valued attributes on the desired side are ignored during the
/// attribute comparison (see module docs).
pub fn detect_drift(doc: &Document, state: &State) -> DriftReport {
    let desired: BTreeMap<&str, &tpt_bastion_ir::Resource> =
        doc.resources.iter().map(|r| (r.id.as_str(), r)).collect();
    let actual: BTreeMap<&str, &tpt_bastion_engine::provider::AppliedResource> =
        state.resources.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut items = Vec::new();

    for (id, res) in desired.iter() {
        match actual.get(id) {
            None => items.push(DriftItem {
                id: (*id).to_string(),
                type_name: Some(res.type_name.clone()),
                kind: DriftKind::MissingResource,
            }),
            Some(applied) => {
                // Compare declared (non-reference) attributes against recorded.
                for (key, want) in &res.attributes {
                    if matches!(want, Value::Reference { .. }) {
                        continue;
                    }
                    let found = applied.attributes.get(key);
                    if Some(want) != found {
                        items.push(DriftItem {
                            id: (*id).to_string(),
                            type_name: Some(res.type_name.clone()),
                            kind: DriftKind::AttributeDrift {
                                attr: key.clone(),
                                expected: Some(want.clone()),
                                actual: found.cloned(),
                            },
                        });
                    }
                }
                // Recorded attributes absent from the document are also drift.
                for key in applied.attributes.keys() {
                    if !res.attributes.contains_key(key) {
                        items.push(DriftItem {
                            id: (*id).to_string(),
                            type_name: Some(res.type_name.clone()),
                            kind: DriftKind::AttributeDrift {
                                attr: key.clone(),
                                expected: None,
                                actual: applied.attributes.get(key).cloned(),
                            },
                        });
                    }
                }
            }
        }
    }

    // Resources applied but no longer declared are orphaned.
    for (id, applied) in actual.iter() {
        if !desired.contains_key(id) {
            items.push(DriftItem {
                id: (*id).to_string(),
                type_name: Some(applied.type_name.clone()),
                kind: DriftKind::ExtraResource,
            });
        }
    }

    DriftReport { items }
}
