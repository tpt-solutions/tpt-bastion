// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Execution plan construction from a dependency-ordered graph.

use crate::error::EngineError;
use crate::state::State;
use bastion_ir::{Document, Resource};
use std::collections::HashMap;

/// A single step in an execution plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// What kind of action to perform.
    pub kind: ActionKind,
    /// The resource id the action targets, if any.
    pub id: String,
    /// The provider-scoped resource type, if known.
    pub type_name: Option<String>,
}

/// The kind of mutation an [`Action`] performs against infrastructure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Create new infrastructure (no prior state for this id).
    Create,
    /// Update existing infrastructure in place.
    Update,
    /// Destroy infrastructure.
    Delete,
    /// Fetch/compute a value without changing infrastructure.
    Read,
}

impl ActionKind {
    /// Short verb used when rendering a plan.
    pub fn verb(&self) -> &'static str {
        match self {
            ActionKind::Create => "create",
            ActionKind::Update => "update",
            ActionKind::Delete => "delete",
            ActionKind::Read => "read",
        }
    }
}

/// An ordered list of [`Action`]s to perform, together with a summary.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Actions in dependency order (dependencies first).
    pub actions: Vec<Action>,
    /// Number of create actions.
    pub to_create: usize,
    /// Number of update actions.
    pub to_update: usize,
    /// Number of delete actions.
    pub to_delete: usize,
}

impl Plan {
    /// Build a plan from a dependency-ordered list of node ids.
    ///
    /// Node ids that are resource ids become actions; output nodes are derived
    /// at apply time and produce no action. Resources are diffed against
    /// `prior` state so the plan contains [`ActionKind::Create`],
    /// [`ActionKind::Update`], and [`ActionKind::Delete`] actions as
    /// appropriate. Passing an empty state (the default) plans every resource
    /// as a create.
    pub fn from_order(
        doc: &Document,
        order: &[String],
        prior: &State,
    ) -> Result<Plan, EngineError> {
        let by_id: HashMap<&str, &Resource> =
            doc.resources.iter().map(|r| (r.id.as_str(), r)).collect();
        let prior_ids: std::collections::HashSet<&str> =
            prior.resources.iter().map(|r| r.id.as_str()).collect();

        let mut actions = Vec::with_capacity(order.len());
        for node in order {
            if let Some(res) = by_id.get(node.as_str()) {
                let kind = if prior_ids.contains(res.id.as_str()) {
                    ActionKind::Update
                } else {
                    ActionKind::Create
                };
                actions.push(Action {
                    kind,
                    id: res.id.clone(),
                    type_name: Some(res.type_name.clone()),
                });
            }
            // Output nodes are intentionally skipped: they are computed, not
            // applied, once the resources they reference exist.
        }

        // Resources present in prior state but absent from the document are
        // planned for deletion, in reverse topological order so that dependents
        // are destroyed before the resources they depend on.
        for prior_res in prior.resources.iter().rev() {
            if !by_id.contains_key(prior_res.id.as_str()) {
                actions.push(Action {
                    kind: ActionKind::Delete,
                    id: prior_res.id.clone(),
                    type_name: Some(prior_res.type_name.clone()),
                });
            }
        }

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

        Ok(Plan {
            actions,
            to_create,
            to_update,
            to_delete,
        })
    }

    /// Render the plan as a human-readable, dry-run-style summary.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.actions.is_empty() {
            out.push_str("Plan: no actions (document has no resources).\n");
            return out;
        }
        out.push_str(&format!(
            "Plan: {} to create, {} to update, {} to delete.\n",
            self.to_create, self.to_update, self.to_delete
        ));
        out.push('\n');
        for (i, action) in self.actions.iter().enumerate() {
            let ty = action.type_name.as_deref().unwrap_or("?");
            out.push_str(&format!(
                "  {:>2}. {} {} ({})\n",
                i + 1,
                action.kind.verb(),
                action.id,
                ty
            ));
        }
        out
    }
}
