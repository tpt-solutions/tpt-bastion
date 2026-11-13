// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Refresh recorded state from the real infrastructure.
//!
//! Refresh asks the provider to `read` every resource in the state. It does not
//! change any infrastructure. The result is an updated state that matches what
//! the provider reports, which later plans and drift checks then compare with the
//! document.
//!
//! A provider error stops the refresh and is returned to the caller. The state is
//! not changed on error, so a transient failure can never look like a deleted
//! resource.

use crate::error::EngineError;
use crate::provider::{AppliedResource, Provider};
use crate::state::State;

/// What refresh found for one recorded resource.
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshOutcome {
    /// The provider reports the same attributes as the recorded state.
    Unchanged,
    /// The provider reports different attributes. The new record is kept.
    Updated {
        /// Attribute names whose values differ, or that appear or disappear.
        changed_attributes: Vec<String>,
    },
    /// The provider reports the resource no longer exists. It is removed from state.
    Missing,
}

/// The per-resource outcomes of a refresh, in the order of the prior state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RefreshReport {
    /// `(resource id, outcome)` pairs.
    pub outcomes: Vec<(String, RefreshOutcome)>,
}

impl RefreshReport {
    /// Number of resources whose recorded state no longer matches reality.
    pub fn drifted(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|(_, o)| !matches!(o, RefreshOutcome::Unchanged))
            .count()
    }
}

/// Attribute names whose values differ between `recorded` and `live`.
fn changed_attributes(recorded: &AppliedResource, live: &AppliedResource) -> Vec<String> {
    let mut names: Vec<String> = recorded
        .attributes
        .keys()
        .chain(live.attributes.keys())
        .filter(|k| recorded.attributes.get(*k) != live.attributes.get(*k))
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Read every resource in `prior` through `provider`, returning the refreshed
/// state and a report of what changed.
pub fn refresh_state(
    prior: &State,
    provider: &dyn Provider,
) -> Result<(State, RefreshReport), EngineError> {
    let mut resources = Vec::with_capacity(prior.resources.len());
    let mut report = RefreshReport::default();

    for recorded in &prior.resources {
        match provider.read(recorded)? {
            None => {
                report
                    .outcomes
                    .push((recorded.id.clone(), RefreshOutcome::Missing));
            }
            Some(live) => {
                let changed = changed_attributes(recorded, &live);
                if changed.is_empty() {
                    report
                        .outcomes
                        .push((recorded.id.clone(), RefreshOutcome::Unchanged));
                    resources.push(recorded.clone());
                } else {
                    report.outcomes.push((
                        recorded.id.clone(),
                        RefreshOutcome::Updated {
                            changed_attributes: changed,
                        },
                    ));
                    resources.push(live);
                }
            }
        }
    }

    Ok((State::from_applied(resources), report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tpt_bastion_ir::{Resource, Value};

    fn record(id: &str, acl: &str) -> AppliedResource {
        let mut attributes = BTreeMap::new();
        attributes.insert("acl".to_string(), Value::String(acl.into()));
        AppliedResource {
            id: id.into(),
            type_name: "aws_s3_bucket".into(),
            attributes,
        }
    }

    /// Reports a fixed live view: `gone` is missing, `changed` has a new acl,
    /// `broken` fails, and everything else matches the recorded state.
    struct LiveView {
        gone: &'static str,
        changed: &'static str,
        broken: &'static str,
    }

    impl Provider for LiveView {
        fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
            Err(EngineError::Provider(format!(
                "unexpected create of {}",
                r.id
            )))
        }
        fn destroy(&self, _: &AppliedResource) -> Result<(), EngineError> {
            Ok(())
        }
        fn read(&self, current: &AppliedResource) -> Result<Option<AppliedResource>, EngineError> {
            if current.id == self.gone {
                return Ok(None);
            }
            if current.id == self.broken {
                return Err(EngineError::Provider("timeout".into()));
            }
            let mut live = current.clone();
            if current.id == self.changed {
                live.attributes
                    .insert("acl".into(), Value::String("public".into()));
            }
            Ok(Some(live))
        }
    }

    fn prior() -> State {
        State::from_applied(vec![
            record("aws_s3_bucket.a", "private"),
            record("aws_s3_bucket.b", "private"),
            record("aws_s3_bucket.c", "private"),
        ])
    }

    #[test]
    fn reports_unchanged_updated_and_missing() {
        let provider = LiveView {
            gone: "aws_s3_bucket.c",
            changed: "aws_s3_bucket.b",
            broken: "",
        };
        let (state, report) = refresh_state(&prior(), &provider).unwrap();

        assert_eq!(state.resources.len(), 2, "missing resource is dropped");
        assert_eq!(report.drifted(), 2);
        assert_eq!(
            report.outcomes[0],
            ("aws_s3_bucket.a".to_string(), RefreshOutcome::Unchanged)
        );
        assert_eq!(
            report.outcomes[1],
            (
                "aws_s3_bucket.b".to_string(),
                RefreshOutcome::Updated {
                    changed_attributes: vec!["acl".to_string()]
                }
            )
        );
        assert_eq!(
            report.outcomes[2],
            ("aws_s3_bucket.c".to_string(), RefreshOutcome::Missing)
        );
        let b = state
            .resources
            .iter()
            .find(|r| r.id == "aws_s3_bucket.b")
            .unwrap();
        assert_eq!(
            b.attributes.get("acl"),
            Some(&Value::String("public".into()))
        );
    }

    #[test]
    fn provider_error_aborts_without_dropping_anything() {
        let provider = LiveView {
            gone: "",
            changed: "",
            broken: "aws_s3_bucket.b",
        };
        let result = refresh_state(&prior(), &provider);
        assert!(matches!(result, Err(EngineError::Provider(_))));
    }
}
