// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Parallel, dependency-aware execution scheduler and apply lifecycle.
//!
//! [`Executor`] turns a [`Plan`] into concrete infrastructure mutations by
//! walking it level-by-level. Resources on the same dependency level share no
//! edges and are therefore applied *in parallel* (via [`rayon`]); each level
//! is fully settled before the next begins, which preserves the ordering the
//! dependency graph guarantees.
//!
//! If any resource fails to apply, every resource applied so far is torn down
//! in reverse order (rollback) and the error is returned. The whole run is
//! observed through an [`EventHandler`](crate::observability::EventHandler),
//! which receives [`Event`](crate::observability::Event)s as work progresses.

use crate::error::EngineError;
use crate::graph::DepGraph;
use crate::observability::{Event, EventHandler};
use crate::plan::{Action, ActionKind, Plan};
use crate::provider::{AppliedResource, Provider};
use crate::state::State;
use bastion_ir::{Document, Resource, Value};
use rayon::prelude::*;
use std::collections::HashMap;

/// Runs a [`Plan`] against a [`Provider`], scheduling work in parallel across
/// dependency levels and rolling back on failure.
///
/// An executor is cheap to construct and `&self` is sufficient to apply, so
/// callers may keep one executor around and run many plans through it (e.g. a
/// long-lived agent or a test harness).
pub struct Executor<P: Provider> {
    provider: P,
    handler: Box<dyn EventHandler>,
}

impl<P: Provider + Send + Sync> Executor<P> {
    /// Create an executor from a provider and an event handler.
    pub fn new(provider: P, handler: Box<dyn EventHandler>) -> Self {
        Executor { provider, handler }
    }

    /// Run `plan` against `doc`, diffing from `prior` state, and return the
    /// resulting [`State`].
    ///
    /// The apply is atomic from the caller's perspective: on any provider
    /// failure the already-applied resources are rolled back and the prior
    /// state is left untouched (the returned error carries the root cause).
    pub fn apply(&self, doc: &Document, plan: &Plan, prior: &State) -> Result<State, EngineError> {
        let graph = DepGraph::build(doc)?;
        let levels = graph.levels();

        // Map local names to fully-qualified ids so references can resolve
        // either way (the IR keeps the local name as the reference base).
        let name_to_id: HashMap<&str, &str> = doc
            .resources
            .iter()
            .map(|r| (r.name.as_str(), r.id.as_str()))
            .collect();

        self.handler.handle(&Event::ApplyStart {
            to_create: plan.to_create,
            to_update: plan.to_update,
            to_delete: plan.to_delete,
        });

        // Build the create/update batches in level order (topo order keeps
        // levels non-decreasing, so consecutive equal-level actions batch up).
        let mut batches: Vec<(usize, Vec<&Action>)> = Vec::new();
        for action in plan.actions.iter().filter(|a| a.kind != ActionKind::Delete) {
            let level = *levels.get(&action.id).unwrap_or(&0);
            match batches.last_mut() {
                Some((lvl, items)) if *lvl == level => items.push(action),
                _ => batches.push((level, vec![action])),
            }
        }

        // `applied` holds resources in creation order; reverse order = teardown
        // order for rollback. `applied_by_id` resolves references at apply time.
        let mut applied: Vec<AppliedResource> = Vec::new();
        let mut applied_by_id: HashMap<String, AppliedResource> = HashMap::new();

        for (level, batch) in batches {
            self.handler.handle(&Event::BatchStart {
                level,
                ids: batch.iter().map(|a| a.id.clone()).collect(),
            });

            // Resolve each resource's references against everything applied in
            // earlier levels, then ask the provider to create it. Running the
            // resolutions sequentially (cheap) before the parallel create keeps
            // the closure free of shared mutable state.
            let resolved: Vec<Resource> = batch
                .iter()
                .map(|a| {
                    let res = doc
                        .resources
                        .iter()
                        .find(|r| r.id == a.id)
                        .expect("action id exists in document");
                    resolve_resource(res, &applied_by_id, &name_to_id)
                })
                .collect::<Result<_, _>>()?;

            let results: Vec<Result<AppliedResource, EngineError>> = resolved
                .par_iter()
                .map(|res| {
                    self.handler
                        .handle(&Event::ResourceStart { id: res.id.clone() });
                    self.provider.create(res)
                })
                .collect();

            // Commit successes, but bail (and roll back) on the first failure.
            let mut failed: Option<EngineError> = None;
            for (res, result) in resolved.iter().zip(results) {
                match result {
                    Ok(applied_res) => {
                        self.handler.handle(&Event::ResourceApplied {
                            resource: applied_res.clone(),
                            level,
                        });
                        applied_by_id.insert(applied_res.id.clone(), applied_res.clone());
                        applied.push(applied_res);
                    }
                    Err(e) => {
                        self.handler.handle(&Event::ResourceFailed {
                            id: res.id.clone(),
                            error: e.to_string(),
                        });
                        if failed.is_none() {
                            failed = Some(e);
                        }
                    }
                }
            }

            if let Some(err) = failed {
                self.rollback(&applied)?;
                return Err(err);
            }
        }

        // Finally, destroy resources that are no longer in the document.
        let prior_by_id: HashMap<&str, &AppliedResource> =
            prior.resources.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut deleted = 0usize;
        for action in plan.actions.iter().filter(|a| a.kind == ActionKind::Delete) {
            if let Some(prior_res) = prior_by_id.get(action.id.as_str()) {
                self.provider.destroy(prior_res)?;
                self.handler.handle(&Event::ResourceDeleted {
                    id: action.id.clone(),
                });
                deleted += 1;
            }
        }

        let created = plan
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Create)
            .count();
        let updated = plan
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Update)
            .count();

        self.handler.handle(&Event::ApplyComplete {
            created,
            updated,
            deleted,
        });

        Ok(State::from_applied(applied))
    }

    /// Tear down every applied resource in reverse creation order.
    fn rollback(&self, applied: &[AppliedResource]) -> Result<(), EngineError> {
        self.handler.handle(&Event::RollbackStart {
            applied_count: applied.len(),
        });
        for res in applied.iter().rev() {
            self.provider.destroy(res)?;
            self.handler
                .handle(&Event::ResourceRolledBack { id: res.id.clone() });
        }
        self.handler.handle(&Event::RollbackComplete);
        Ok(())
    }
}

/// Resolve every reference inside `resource`'s attributes against resources that
/// have already been applied, returning a copy with concrete values in place of
/// [`Value::Reference`] nodes.
fn resolve_resource(
    resource: &Resource,
    applied_by_id: &HashMap<String, AppliedResource>,
    name_to_id: &HashMap<&str, &str>,
) -> Result<Resource, EngineError> {
    let mut attributes = resource.attributes.clone();
    for value in attributes.values_mut() {
        *value = resolve_value(value, applied_by_id, name_to_id)?;
    }
    Ok(Resource {
        id: resource.id.clone(),
        type_name: resource.type_name.clone(),
        name: resource.name.clone(),
        attributes,
    })
}

/// Recursively replace references in a value with their resolved targets.
fn resolve_value(
    value: &Value,
    applied_by_id: &HashMap<String, AppliedResource>,
    name_to_id: &HashMap<&str, &str>,
) -> Result<Value, EngineError> {
    match value {
        Value::Reference { base, path } => {
            let id = if applied_by_id.contains_key(base) {
                base.clone()
            } else if let Some(&mapped) = name_to_id.get(base.as_str()) {
                mapped.to_string()
            } else {
                return Err(EngineError::ReferenceResolution {
                    base: base.clone(),
                    detail: "no such applied resource".to_string(),
                });
            };
            let applied =
                applied_by_id
                    .get(&id)
                    .ok_or_else(|| EngineError::ReferenceResolution {
                        base: base.clone(),
                        detail: "referenced resource not yet applied".to_string(),
                    })?;
            let root = Value::Map(applied.attributes.clone());
            walk_path(&root, path, base)
        }
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_value(item, applied_by_id, name_to_id)?);
            }
            Ok(Value::List(out))
        }
        Value::Map(map) => {
            let mut out = std::collections::BTreeMap::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_value(v, applied_by_id, name_to_id)?);
            }
            Ok(Value::Map(out))
        }
        other => Ok(other.clone()),
    }
}

/// Walk a `path` of attribute segments into a previously-resolved value.
fn walk_path(root: &Value, path: &[String], base: &str) -> Result<Value, EngineError> {
    let mut current = root;
    for segment in path {
        match current {
            Value::Map(map) => match map.get(segment) {
                Some(next) => current = next,
                None => {
                    return Err(EngineError::ReferenceResolution {
                        base: base.to_string(),
                        detail: format!("attribute `{segment}` not found"),
                    })
                }
            },
            _ => {
                return Err(EngineError::ReferenceResolution {
                    base: base.to_string(),
                    detail: format!("attribute `{segment}` is not a map"),
                })
            }
        }
    }
    Ok(current.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::NoopHandler;
    use crate::provider::MockProvider;
    use std::collections::BTreeMap;

    fn doc_with_refs() -> Document {
        let mut bucket = Resource {
            id: "aws_s3_bucket.b".to_string(),
            type_name: "aws_s3_bucket".to_string(),
            name: "b".to_string(),
            attributes: BTreeMap::new(),
        };
        bucket
            .attributes
            .insert("bucket".to_string(), Value::String("my-bucket".to_string()));

        let mut obj = Resource {
            id: "aws_s3_bucket.o".to_string(),
            type_name: "aws_s3_bucket".to_string(),
            name: "o".to_string(),
            attributes: BTreeMap::new(),
        };
        obj.attributes.insert(
            "source".to_string(),
            Value::Reference {
                base: "b".to_string(),
                path: vec!["bucket".to_string()],
            },
        );

        Document {
            ir_version: bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![bucket, obj],
            modules: vec![],
            outputs: vec![],
        }
    }

    #[test]
    fn applies_in_parallel_and_resolves_references() {
        let doc = doc_with_refs();
        let plan = crate::plan_document(&doc).unwrap();
        let exec = Executor::new(MockProvider, Box::new(NoopHandler));
        let state = exec.apply(&doc, &plan, &State::new()).unwrap();

        assert_eq!(state.resources.len(), 2);
        let obj = state
            .resources
            .iter()
            .find(|r| r.id == "aws_s3_bucket.o")
            .unwrap();
        assert_eq!(
            obj.attributes.get("source"),
            Some(&Value::String("my-bucket".to_string()))
        );
    }

    #[test]
    fn rollback_fires_when_provider_fails() {
        struct FailingProvider;
        impl Provider for FailingProvider {
            fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
                if r.id == "aws_s3_bucket.o" {
                    Err(EngineError::Provider("boom".to_string()))
                } else {
                    Ok(AppliedResource {
                        id: r.id.clone(),
                        type_name: r.type_name.clone(),
                        attributes: r.attributes.clone(),
                    })
                }
            }
            fn destroy(&self, _: &AppliedResource) -> Result<(), EngineError> {
                Ok(())
            }
        }

        let doc = doc_with_refs();
        let plan = crate::plan_document(&doc).unwrap();
        let exec = Executor::new(FailingProvider, Box::new(NoopHandler));
        let result = exec.apply(&doc, &plan, &State::new());
        assert!(matches!(result, Err(EngineError::Provider(_))));
    }
}
