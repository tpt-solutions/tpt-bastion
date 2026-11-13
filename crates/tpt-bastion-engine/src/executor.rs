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
use rayon::prelude::*;
use std::collections::HashMap;
use tpt_bastion_ir::{Document, Resource, Value};

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
    /// Creates and updates run level by level. No-op resources keep their prior
    /// record and are not touched. If anything fails, earlier changes are undone:
    /// created resources are destroyed and updated resources are reverted to
    /// their prior attributes. The returned error carries the root cause.
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

        let prior_by_id: HashMap<&str, &AppliedResource> =
            prior.resources.iter().map(|r| (r.id.as_str(), r)).collect();

        self.handler.handle(&Event::ApplyStart {
            to_create: plan.to_create,
            to_update: plan.to_update,
            to_delete: plan.to_delete,
        });

        // Build the create/update batches in level order (topo order keeps
        // levels non-decreasing, so consecutive equal-level actions batch up).
        let mut batches: Vec<(usize, Vec<&Action>)> = Vec::new();
        for action in plan.actions.iter().filter(|a| {
            matches!(
                a.kind,
                ActionKind::Create | ActionKind::Update | ActionKind::Replace
            )
        }) {
            let level = *levels.get(&action.id).unwrap_or(&0);
            match batches.last_mut() {
                Some((lvl, items)) if *lvl == level => items.push(action),
                _ => batches.push((level, vec![action])),
            }
        }

        // `result` is the state returned on success. `undo` records how to
        // reverse each change, in the order the changes were made.
        // `applied_by_id` resolves references at apply time.
        let mut result: Vec<AppliedResource> = Vec::new();
        let mut undo: Vec<Undo> = Vec::new();
        let mut applied_by_id: HashMap<String, AppliedResource> = HashMap::new();

        // Unchanged resources keep their prior record and are visible to
        // references immediately. They are never touched or undone.
        for action in plan.actions.iter().filter(|a| a.kind == ActionKind::NoOp) {
            if let Some(prior_res) = prior_by_id.get(action.id.as_str()) {
                applied_by_id.insert(action.id.clone(), (*prior_res).clone());
                result.push((*prior_res).clone());
            }
        }

        for (level, batch) in batches {
            self.handler.handle(&Event::BatchStart {
                level,
                ids: batch.iter().map(|a| a.id.clone()).collect(),
            });

            // Resolve each resource's references against everything applied in
            // earlier levels, then ask the provider to create or update it.
            // Running the resolutions sequentially (cheap) before the parallel
            // calls keeps the closure free of shared mutable state.
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

            // Each outcome pairs the new record with the prior record it replaced
            // (updates only), so a later failure can revert the change.
            type Outcome = Result<(AppliedResource, Change), EngineError>;
            let results: Vec<Outcome> = resolved
                .par_iter()
                .zip(batch.par_iter())
                .map(|(res, action)| {
                    self.handler
                        .handle(&Event::ResourceStart { id: res.id.clone() });
                    let current_record = || {
                        prior_by_id
                            .get(res.id.as_str())
                            .copied()
                            .cloned()
                            .ok_or_else(|| {
                                EngineError::Provider(format!(
                                    "no prior record for `{}` to change",
                                    res.id
                                ))
                            })
                    };
                    match action.kind {
                        ActionKind::Update => {
                            let current = current_record()?;
                            let updated = self.provider.update(&current, res)?;
                            Ok((updated, Change::Updated(current)))
                        }
                        ActionKind::Replace => {
                            // Destroy first: the id is shared, so the old resource
                            // must be gone before the new one is created.
                            let current = current_record()?;
                            self.provider.destroy(&current)?;
                            match self.provider.create(res) {
                                Ok(created) => Ok((created, Change::Replaced(current))),
                                Err(e) => {
                                    // Put the old resource back so a failed replace
                                    // leaves the infrastructure as it was.
                                    let restore = resource_from_applied(&current);
                                    match self.provider.create(&restore) {
                                        Ok(_) => Err(e),
                                        Err(r) => Err(EngineError::Provider(format!(
                                            "{e}; restoring `{}` also failed: {r}",
                                            res.id
                                        ))),
                                    }
                                }
                            }
                        }
                        _ => self
                            .provider
                            .create(res)
                            .map(|created| (created, Change::Created)),
                    }
                })
                .collect();

            // Commit successes, but bail (and undo) on the first failure.
            let mut failed: Option<EngineError> = None;
            for (res, outcome) in resolved.iter().zip(results) {
                match outcome {
                    Ok((applied_res, change)) => {
                        self.handler.handle(&Event::ResourceApplied {
                            resource: applied_res.clone(),
                            level,
                        });
                        applied_by_id.insert(applied_res.id.clone(), applied_res.clone());
                        undo.push(match change {
                            Change::Created => Undo::Destroy(applied_res.clone()),
                            Change::Updated(before) => Undo::Revert {
                                before,
                                after: applied_res.clone(),
                            },
                            Change::Replaced(before) => Undo::Replaced {
                                before,
                                after: applied_res.clone(),
                            },
                        });
                        result.push(applied_res);
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
                return Err(self.undo_changes(&undo, err));
            }
        }

        // Finally, destroy resources that are no longer in the document. If one
        // fails, the resources already destroyed are recreated from their prior
        // records, so the infrastructure matches the still-current state.
        let mut deleted = 0usize;
        for action in plan.actions.iter().filter(|a| a.kind == ActionKind::Delete) {
            if let Some(prior_res) = prior_by_id.get(action.id.as_str()) {
                if let Err(e) = self.provider.destroy(prior_res) {
                    self.handler.handle(&Event::ResourceFailed {
                        id: action.id.clone(),
                        error: e.to_string(),
                    });
                    for destroyed in plan
                        .actions
                        .iter()
                        .filter(|a| a.kind == ActionKind::Delete)
                        .take(deleted)
                        .filter_map(|a| prior_by_id.get(a.id.as_str()))
                    {
                        undo.push(Undo::Recreate((*destroyed).clone()));
                    }
                    return Err(self.undo_changes(&undo, e));
                }
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

        Ok(State::from_applied(result))
    }

    /// Reverse the recorded changes, newest first, and return the error to
    /// report. If a reversal itself fails, the failure is added to the message
    /// so the root cause is not lost.
    fn undo_changes(&self, undo: &[Undo], cause: EngineError) -> EngineError {
        self.handler.handle(&Event::RollbackStart {
            applied_count: undo.len(),
        });
        let mut failures: Vec<String> = Vec::new();
        for step in undo.iter().rev() {
            let (id, outcome) = match step {
                Undo::Destroy(res) => (res.id.clone(), self.provider.destroy(res)),
                Undo::Revert { before, after } => {
                    let desired = resource_from_applied(before);
                    (
                        before.id.clone(),
                        self.provider.update(after, &desired).map(|_| ()),
                    )
                }
                Undo::Recreate(before) => {
                    let desired = resource_from_applied(before);
                    (
                        before.id.clone(),
                        self.provider.create(&desired).map(|_| ()),
                    )
                }
                Undo::Replaced { before, after } => {
                    let desired = resource_from_applied(before);
                    let outcome = self
                        .provider
                        .destroy(after)
                        .and_then(|_| self.provider.create(&desired).map(|_| ()));
                    (before.id.clone(), outcome)
                }
            };
            match outcome {
                Ok(()) => self.handler.handle(&Event::ResourceRolledBack { id }),
                Err(e) => failures.push(format!("{id}: {e}")),
            }
        }
        self.handler.handle(&Event::RollbackComplete);
        if failures.is_empty() {
            cause
        } else {
            EngineError::Provider(format!(
                "{cause}; rollback incomplete: {}",
                failures.join("; ")
            ))
        }
    }
}

/// How to reverse one change made during an apply.
enum Undo {
    /// A resource was created: destroy it.
    Destroy(AppliedResource),
    /// A resource was updated: send it back to `before`.
    Revert {
        before: AppliedResource,
        after: AppliedResource,
    },
    /// A resource was destroyed: create it again from its prior record.
    Recreate(AppliedResource),
    /// A resource was replaced: destroy `after`, then recreate `before`.
    Replaced {
        before: AppliedResource,
        after: AppliedResource,
    },
}

/// What one successful action did, and the record it replaced if any.
enum Change {
    Created,
    Updated(AppliedResource),
    Replaced(AppliedResource),
}

/// Rebuild a desired [`Resource`] from a recorded one, used to revert an update.
fn resource_from_applied(applied: &AppliedResource) -> Resource {
    let name = applied
        .id
        .split_once('.')
        .map(|(_, n)| n)
        .unwrap_or(&applied.id)
        .to_string();
    Resource {
        id: applied.id.clone(),
        type_name: applied.type_name.clone(),
        name,
        attributes: applied.attributes.clone(),
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
            ir_version: tpt_bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![bucket, obj],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
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

#[cfg(test)]
mod update_tests {
    use super::*;
    use crate::observability::NoopHandler;
    use crate::plan_document_with_state;
    use crate::provider::{MockProvider, ProviderRouter};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tpt_bastion_ir::IR_VERSION;

    fn bucket(acl: &str) -> Resource {
        let mut attributes = BTreeMap::new();
        attributes.insert("bucket".to_string(), Value::String("b1".into()));
        attributes.insert("acl".to_string(), Value::String(acl.into()));
        Resource {
            id: "aws_s3_bucket.b".into(),
            type_name: "aws_s3_bucket".into(),
            name: "b".into(),
            attributes,
        }
    }

    fn doc(acl: &str) -> Document {
        Document {
            ir_version: IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![bucket(acl)],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
        }
    }

    fn applied(r: &Resource) -> AppliedResource {
        AppliedResource {
            id: r.id.clone(),
            type_name: r.type_name.clone(),
            attributes: r.attributes.clone(),
        }
    }

    /// Counts each operation; `fail_update` simulates a provider refusing an update.
    #[derive(Default)]
    struct Counting {
        creates: AtomicUsize,
        updates: AtomicUsize,
        destroys: AtomicUsize,
        fail_update: bool,
    }

    impl Provider for Counting {
        fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            Ok(applied(r))
        }
        fn update(
            &self,
            _current: &AppliedResource,
            desired: &Resource,
        ) -> Result<AppliedResource, EngineError> {
            self.updates.fetch_add(1, Ordering::SeqCst);
            if self.fail_update {
                return Err(EngineError::Provider("update refused".into()));
            }
            Ok(applied(desired))
        }
        fn destroy(&self, _: &AppliedResource) -> Result<(), EngineError> {
            self.destroys.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn initial_state(acl: &str) -> State {
        let d = doc(acl);
        let plan = crate::plan_document(&d).unwrap();
        Executor::new(MockProvider, Box::new(NoopHandler))
            .apply(&d, &plan, &State::new())
            .unwrap()
    }

    #[test]
    fn unchanged_resource_is_not_touched() {
        let prior = initial_state("private");
        let d = doc("private");
        let plan = plan_document_with_state(&d, &prior).unwrap();
        assert_eq!(plan.unchanged, 1);
        assert_eq!(plan.to_update, 0);

        let exec = Executor::new(Counting::default(), Box::new(NoopHandler));
        let state = exec.apply(&d, &plan, &prior).unwrap();
        assert_eq!(exec.provider.creates.load(Ordering::SeqCst), 0);
        assert_eq!(exec.provider.updates.load(Ordering::SeqCst), 0);
        assert_eq!(state.resources.len(), 1);
    }

    #[test]
    fn changed_attribute_calls_update_not_create() {
        let prior = initial_state("private");
        let d = doc("public");
        let plan = plan_document_with_state(&d, &prior).unwrap();
        assert_eq!(plan.to_update, 1);

        let exec = Executor::new(Counting::default(), Box::new(NoopHandler));
        let state = exec.apply(&d, &plan, &prior).unwrap();
        assert_eq!(exec.provider.updates.load(Ordering::SeqCst), 1);
        assert_eq!(exec.provider.creates.load(Ordering::SeqCst), 0);
        assert_eq!(
            state.resources[0].attributes.get("acl"),
            Some(&Value::String("public".into()))
        );
    }

    #[test]
    fn failed_update_does_not_destroy_the_resource() {
        let prior = initial_state("private");
        let d = doc("public");
        let plan = plan_document_with_state(&d, &prior).unwrap();

        let provider = Counting {
            fail_update: true,
            ..Counting::default()
        };
        let exec = Executor::new(provider, Box::new(NoopHandler));
        let result = exec.apply(&d, &plan, &prior);
        assert!(matches!(result, Err(EngineError::Provider(_))));
        assert_eq!(exec.provider.destroys.load(Ordering::SeqCst), 0);
    }

    /// Stamps every created resource with the name of the provider that made it.
    struct Tagged(&'static str);

    impl Provider for Tagged {
        fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
            let mut a = applied(r);
            a.attributes
                .insert("provider".into(), Value::String(self.0.into()));
            Ok(a)
        }
        fn destroy(&self, _: &AppliedResource) -> Result<(), EngineError> {
            Ok(())
        }
    }

    #[test]
    fn failed_delete_recreates_resources_already_destroyed() {
        /// Refuses to destroy one resource and counts recreations.
        struct RefuseDestroy {
            refuse: &'static str,
            creates: AtomicUsize,
        }
        impl Provider for RefuseDestroy {
            fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
                self.creates.fetch_add(1, Ordering::SeqCst);
                Ok(applied(r))
            }
            fn destroy(&self, a: &AppliedResource) -> Result<(), EngineError> {
                if a.id == self.refuse {
                    Err(EngineError::Provider("destroy refused".into()))
                } else {
                    Ok(())
                }
            }
        }

        let record = |id: &str| AppliedResource {
            id: id.into(),
            type_name: "aws_s3_bucket".into(),
            attributes: BTreeMap::new(),
        };
        let prior = State::from_applied(vec![record("aws_s3_bucket.a"), record("aws_s3_bucket.b")]);
        // The document declares nothing, so both prior resources are deleted.
        // Deletes run in reverse order: `b` is destroyed first, then `a` fails.
        let empty = Document {
            ir_version: IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
        };
        let plan = plan_document_with_state(&empty, &prior).unwrap();

        let exec = Executor::new(
            RefuseDestroy {
                refuse: "aws_s3_bucket.a",
                creates: AtomicUsize::new(0),
            },
            Box::new(NoopHandler),
        );
        assert!(exec.apply(&empty, &plan, &prior).is_err());
        assert_eq!(
            exec.provider.creates.load(Ordering::SeqCst),
            1,
            "`b` was destroyed before the failure and must be recreated"
        );
    }

    /// Declares `acl` immutable. Counts operations; `fail_create` refuses to create
    /// the new version, to exercise the restore path.
    #[derive(Default)]
    struct Immutable {
        creates: AtomicUsize,
        destroys: AtomicUsize,
        fail_create: bool,
    }

    impl Provider for Immutable {
        fn create(&self, r: &Resource) -> Result<AppliedResource, EngineError> {
            let n = self.creates.fetch_add(1, Ordering::SeqCst);
            if self.fail_create && n == 0 {
                return Err(EngineError::Provider("create refused".into()));
            }
            Ok(applied(r))
        }
        fn destroy(&self, _: &AppliedResource) -> Result<(), EngineError> {
            self.destroys.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn requires_replace(&self, _type: &str, attribute: &str) -> bool {
            attribute == "acl"
        }
    }

    fn replace_plan(prior: &State, d: &Document) -> Plan {
        let order = vec!["aws_s3_bucket.b".to_string()];
        Plan::from_order_with_rules(d, &order, prior, &|t, a| {
            Immutable::default().requires_replace(t, a)
        })
        .unwrap()
    }

    #[test]
    fn immutable_change_replaces_the_resource() {
        let prior = initial_state("private");
        let d = doc("public");
        let plan = replace_plan(&prior, &d);
        assert_eq!(plan.actions[0].kind, ActionKind::Replace);

        let exec = Executor::new(Immutable::default(), Box::new(NoopHandler));
        let state = exec.apply(&d, &plan, &prior).unwrap();
        assert_eq!(exec.provider.destroys.load(Ordering::SeqCst), 1);
        assert_eq!(exec.provider.creates.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.resources[0].attributes.get("acl"),
            Some(&Value::String("public".into()))
        );
    }

    #[test]
    fn failed_replace_restores_the_old_resource() {
        let prior = initial_state("private");
        let d = doc("public");
        let plan = replace_plan(&prior, &d);

        let exec = Executor::new(
            Immutable {
                fail_create: true,
                ..Immutable::default()
            },
            Box::new(NoopHandler),
        );
        assert!(exec.apply(&d, &plan, &prior).is_err());
        // The old resource was destroyed, then recreated to restore it.
        assert_eq!(exec.provider.destroys.load(Ordering::SeqCst), 1);
        assert_eq!(exec.provider.creates.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn router_picks_the_longest_matching_prefix() {
        let router = ProviderRouter::new()
            .route("aws_", Box::new(Tagged("aws")))
            .route("aws_s3_", Box::new(Tagged("s3")));

        let s3 = router.create(&bucket("private")).unwrap();
        assert_eq!(
            s3.attributes.get("provider"),
            Some(&Value::String("s3".into()))
        );

        let mut ec2 = bucket("x");
        ec2.id = "aws_instance.i".into();
        ec2.type_name = "aws_instance".into();
        let ec2 = router.create(&ec2).unwrap();
        assert_eq!(
            ec2.attributes.get("provider"),
            Some(&Value::String("aws".into()))
        );

        let mut gcp = bucket("x");
        gcp.type_name = "gcp_storage_bucket".into();
        assert!(matches!(router.create(&gcp), Err(EngineError::Provider(_))));
    }
}
