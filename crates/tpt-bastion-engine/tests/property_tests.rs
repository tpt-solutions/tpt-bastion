// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property / end-to-end tests for the engine plan+apply pipeline.
//!
//! These stand in for Phase 8's "end-to-end plan/apply test scenarios" and
//! "property tests for the IR": across resource counts (including the empty and
//! single-resource edge cases) a compiled document must round-trip through JSON,
//! plan only the resources it declares, and apply to a state that — when
//! re-planned — shows updates rather than re-creates.

use std::collections::BTreeMap;
use tpt_bastion_engine::{
    plan_document, plan_document_with_state, Executor, MockProvider, NoopHandler, State,
};
use tpt_bastion_ir::{Document, Resource, Value, IR_VERSION};

fn make_doc(n: usize) -> Document {
    let mut resources = Vec::with_capacity(n);
    for i in 0..n {
        let mut attrs = BTreeMap::new();
        attrs.insert("bucket".to_string(), Value::String(format!("b{i}")));
        resources.push(Resource {
            id: format!("aws_s3_bucket.r{i}"),
            type_name: "aws_s3_bucket".into(),
            name: format!("r{i}"),
            attributes: attrs,
        });
    }
    Document {
        ir_version: IR_VERSION,
        config: None,
        variables: vec![],
        resources,
        modules: vec![],
        outputs: vec![],
    }
}

#[test]
fn ir_round_trips_through_json() {
    for n in 0..8usize {
        let doc = make_doc(n);
        let json = serde_json::to_string(&doc).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back, "IR did not round-trip for n={n}");
    }
}

#[test]
fn plan_only_references_declared_resources() {
    for n in 0..8usize {
        let doc = make_doc(n);
        let plan = plan_document(&doc).unwrap();
        let ids: std::collections::BTreeSet<String> =
            doc.resources.iter().map(|r| r.id.clone()).collect();
        for action in &plan.actions {
            assert!(
                ids.contains(&action.id),
                "plan references undeclared id `{}`",
                action.id
            );
        }
        assert_eq!(plan.to_create, n, "expected {n} creates");
    }
}

#[test]
fn apply_is_idempotent_against_prior_state() {
    for n in 1..8usize {
        let doc = make_doc(n);
        let exec = Executor::new(MockProvider, Box::new(NoopHandler));

        // First apply creates everything.
        let plan1 = plan_document(&doc).unwrap();
        let state1 = exec.apply(&doc, &plan1, &State::new()).unwrap();
        assert_eq!(state1.resources.len(), n, "n={n}");

        // Re-planning an unchanged document against its applied state is a
        // no-op for every resource: nothing is created or updated.
        let plan2 = plan_document_with_state(&doc, &state1).unwrap();
        assert_eq!(plan2.to_create, 0, "n={n}: nothing should re-create");
        assert_eq!(plan2.to_update, 0, "n={n}: nothing should update");
        assert_eq!(plan2.unchanged, n, "n={n}: all should be no-ops");

        // Applying the no-op plan must keep every resource exactly as it was.
        let state2 = exec.apply(&doc, &plan2, &state1).unwrap();
        assert_eq!(state2.resources.len(), n, "n={n}: count stable");
        assert_eq!(state2.resources, state1.resources, "n={n}: state unchanged");
    }
}
