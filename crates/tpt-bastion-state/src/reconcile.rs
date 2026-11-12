// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Automatic drift reconciliation.
//!
//! Given a [`drift::DriftReport`], produce a concrete [`tpt_bastion_engine::Plan`]
//! whose actions, when applied through the engine's [`Executor`], return
//! infrastructure to the state declared by the document. This is the "fix it"
//! half of drift detection: [`detect_and_reconcile`] goes from a desired
//! document and recorded state straight to a ready-to-apply plan.

use crate::drift::{detect_drift, DriftKind, DriftReport};
use crate::state::State;
use tpt_bastion_engine::plan::{Action, ActionKind, Plan};
use tpt_bastion_ir::Document;

/// Turn a [`DriftReport`] into a [`Plan`] that reconciles the drift.
///
/// - [`DriftKind::MissingResource`] → `create`
/// - [`DriftKind::ExtraResource`] → `delete`
/// - [`DriftKind::AttributeDrift`] → `update`
///
/// Create/update actions are emitted before delete actions so the returned plan
/// is ordered compatibly with the engine's apply pipeline.
pub fn reconcile(report: &DriftReport) -> Plan {
    let mut create_update: Vec<Action> = Vec::new();
    let mut delete: Vec<Action> = Vec::new();

    for item in &report.items {
        let (kind, bucket) = match &item.kind {
            DriftKind::MissingResource => (ActionKind::Create, &mut create_update),
            DriftKind::AttributeDrift { .. } => (ActionKind::Update, &mut create_update),
            DriftKind::ExtraResource => (ActionKind::Delete, &mut delete),
        };
        bucket.push(Action {
            kind,
            id: item.id.clone(),
            type_name: item.type_name.clone(),
        });
    }

    let mut actions = Vec::with_capacity(report.items.len());
    actions.extend(create_update);
    actions.extend(delete);

    let to_create = actions
        .iter()
        .filter(|a| a.kind == ActionKind::Create)
        .count();
    let to_update = actions
        .iter()
        .filter(|a| a.kind == ActionKind::Update)
        .count();
    let to_delete = actions
        .iter()
        .filter(|a| a.kind == ActionKind::Delete)
        .count();

    Plan {
        actions,
        to_create,
        to_update,
        to_delete,
    }
}

/// Convenience: detect drift between `doc` and `state`, then build the
/// reconciliation plan. Returns both so callers can show the report and apply
/// the plan.
pub fn detect_and_reconcile(doc: &Document, state: &State) -> (DriftReport, Plan) {
    let report = detect_drift(doc, state);
    let plan = reconcile(&report);
    (report, plan)
}

/// Build a human-readable summary of a reconciliation plan.
pub fn render_reconcile(plan: &Plan) -> String {
    if plan.actions.is_empty() {
        return "Reconcile: infrastructure already matches the document — nothing to do.\n"
            .to_string();
    }
    plan.render()
}
