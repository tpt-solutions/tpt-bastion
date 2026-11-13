// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Execution plan construction from a dependency-ordered graph.
//!
//! A plan diffs every declared resource against the prior state. Only the
//! attributes the document declares are compared: state can hold values the
//! provider computed (such as an ARN) that the document never mentions, and
//! those must not register as changes. An attribute whose value is a reference
//! cannot be compared until apply, so it is reported as known after apply and
//! the resource plans as an update.

use crate::error::EngineError;
use crate::state::State;
use std::collections::{BTreeMap, HashMap};
use tpt_bastion_ir::{Document, Resource, Value};

/// A single step in an execution plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    /// What kind of action to perform.
    pub kind: ActionKind,
    /// The resource id the action targets, if any.
    pub id: String,
    /// The provider-scoped resource type, if known.
    pub type_name: Option<String>,
    /// Attributes that differ from the prior state. Empty for creates and
    /// deletes, and for no-ops.
    pub changes: Vec<AttributeChange>,
}

/// One attribute that differs between the prior state and the document.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeChange {
    /// Attribute name.
    pub name: String,
    /// Value recorded in state, or `None` when the attribute is new.
    pub before: Option<Value>,
    /// Desired value, or `None` when it is known only after apply.
    pub after: Option<Value>,
    /// The desired value is a reference that resolves during apply.
    pub known_after_apply: bool,
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
    /// The resource exists and matches the document. Nothing is applied.
    NoOp,
}

impl ActionKind {
    /// Short verb used when rendering a plan.
    pub fn verb(&self) -> &'static str {
        match self {
            ActionKind::Create => "create",
            ActionKind::Update => "update",
            ActionKind::Delete => "delete",
            ActionKind::Read => "read",
            ActionKind::NoOp => "no-op",
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
    /// Number of resources that already match the document.
    pub unchanged: usize,
}

/// Compare the attributes a document declares against the recorded state.
///
/// Attributes that exist only in state are ignored: they are provider-computed
/// values, not part of the declared configuration.
pub fn diff_attributes(
    desired: &BTreeMap<String, Value>,
    prior: &BTreeMap<String, Value>,
) -> Vec<AttributeChange> {
    let mut changes = Vec::new();
    for (name, value) in desired {
        let before = prior.get(name);
        if value.contains_reference() {
            changes.push(AttributeChange {
                name: name.clone(),
                before: before.cloned(),
                after: None,
                known_after_apply: true,
            });
        } else if before != Some(value) {
            changes.push(AttributeChange {
                name: name.clone(),
                before: before.cloned(),
                after: Some(value.clone()),
                known_after_apply: false,
            });
        }
    }
    changes
}

/// ANSI SGR code for an action verb: green create, yellow update, red delete.
fn verb_color(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::Create => "32",
        ActionKind::Update => "33",
        ActionKind::Delete => "31",
        ActionKind::Read => "36",
        ActionKind::NoOp => "2",
    }
}

/// Wrap `text` in an ANSI color when `color` is set.
fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Render a value for plan output.
fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => format!("{n}"),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Reference { base, path } => {
            let mut out = base.clone();
            for segment in path {
                out.push('.');
                out.push_str(segment);
            }
            out
        }
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(display_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Map(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k} = {}", display_value(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

impl Plan {
    /// Build a plan from a dependency-ordered list of node ids.
    ///
    /// Node ids that are resource ids become actions; output nodes are derived
    /// at apply time and produce no action. Resources are diffed against
    /// `prior` state: a resource with no prior record is a create, one whose
    /// declared attributes differ is an update, and one that matches is a no-op.
    /// Prior resources absent from the document become deletes. Passing an
    /// empty state (the default) plans every resource as a create.
    pub fn from_order(
        doc: &Document,
        order: &[String],
        prior: &State,
    ) -> Result<Plan, EngineError> {
        let by_id: HashMap<&str, &Resource> =
            doc.resources.iter().map(|r| (r.id.as_str(), r)).collect();
        let prior_by_id: HashMap<&str, &crate::provider::AppliedResource> =
            prior.resources.iter().map(|r| (r.id.as_str(), r)).collect();

        let mut actions = Vec::with_capacity(order.len());
        for node in order {
            let Some(res) = by_id.get(node.as_str()) else {
                // Output nodes are intentionally skipped: they are computed, not
                // applied, once the resources they reference exist.
                continue;
            };
            let (kind, changes) = match prior_by_id.get(res.id.as_str()) {
                None => (ActionKind::Create, Vec::new()),
                Some(existing) => {
                    let changes = diff_attributes(&res.attributes, &existing.attributes);
                    if changes.is_empty() {
                        (ActionKind::NoOp, changes)
                    } else {
                        (ActionKind::Update, changes)
                    }
                }
            };
            actions.push(Action {
                kind,
                id: res.id.clone(),
                type_name: Some(res.type_name.clone()),
                changes,
            });
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
                    changes: Vec::new(),
                });
            }
        }

        let count = |kind: ActionKind| actions.iter().filter(|a| a.kind == kind).count();
        Ok(Plan {
            to_create: count(ActionKind::Create),
            to_update: count(ActionKind::Update),
            to_delete: count(ActionKind::Delete),
            unchanged: count(ActionKind::NoOp),
            actions,
        })
    }

    /// Render the plan as a human-readable, dry-run-style summary.
    pub fn render(&self) -> String {
        self.render_with(false)
    }

    /// Render like [`Plan::render`], with ANSI colors on the action verbs.
    /// Callers should use this only when the output is a terminal.
    pub fn render_colored(&self) -> String {
        self.render_with(true)
    }

    fn render_with(&self, color: bool) -> String {
        let mut out = String::new();
        if self.actions.is_empty() {
            out.push_str("Plan: no actions (document has no resources).\n");
            return out;
        }
        out.push_str(&format!(
            "Plan: {} to create, {} to update, {} to delete, {} unchanged.\n",
            self.to_create, self.to_update, self.to_delete, self.unchanged
        ));
        out.push('\n');
        for (i, action) in self.actions.iter().enumerate() {
            let ty = action.type_name.as_deref().unwrap_or("?");
            let verb = paint(action.kind.verb(), verb_color(action.kind), color);
            out.push_str(&format!(
                "  {:>2}. {} {} ({})\n",
                i + 1,
                verb,
                action.id,
                ty
            ));
            for change in &action.changes {
                let line = if change.known_after_apply {
                    let before = change
                        .before
                        .as_ref()
                        .map(|b| format!("{} -> ", display_value(b)))
                        .unwrap_or_default();
                    format!("{}{before}(known after apply)", change.name)
                } else {
                    match (&change.before, &change.after) {
                        (Some(b), Some(a)) => {
                            format!(
                                "{}: {} -> {}",
                                change.name,
                                display_value(b),
                                display_value(a)
                            )
                        }
                        (None, Some(a)) => format!("{}: (new) {}", change.name, display_value(a)),
                        _ => format!("{}: removed", change.name),
                    }
                };
                out.push_str(&format!("        ~ {line}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::AppliedResource;
    use std::collections::BTreeMap;

    fn attrs(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn identical_declared_attributes_are_a_noop() {
        let desired = attrs(&[("acl", Value::String("private".into()))]);
        // A computed attribute the document never declares must not count as a change.
        let prior = attrs(&[
            ("acl", Value::String("private".into())),
            ("arn", Value::String("arn:aws:s3:::b".into())),
        ]);
        assert!(diff_attributes(&desired, &prior).is_empty());
    }

    #[test]
    fn changed_attribute_is_reported_with_before_and_after() {
        let desired = attrs(&[("acl", Value::String("public".into()))]);
        let prior = attrs(&[("acl", Value::String("private".into()))]);
        let changes = diff_attributes(&desired, &prior);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].name, "acl");
        assert_eq!(changes[0].before, Some(Value::String("private".into())));
        assert_eq!(changes[0].after, Some(Value::String("public".into())));
        assert!(!changes[0].known_after_apply);
    }

    #[test]
    fn reference_attribute_is_known_after_apply() {
        let desired = attrs(&[(
            "bucket",
            Value::Reference {
                base: "other".into(),
                path: vec!["bucket".into()],
            },
        )]);
        let changes = diff_attributes(&desired, &BTreeMap::new());
        assert_eq!(changes.len(), 1);
        assert!(changes[0].known_after_apply);
        assert_eq!(changes[0].after, None);
    }

    #[test]
    fn colored_render_only_adds_escape_codes() {
        let plan = Plan {
            actions: vec![Action {
                kind: ActionKind::Create,
                id: "aws_s3_bucket.b".into(),
                type_name: Some("aws_s3_bucket".into()),
                changes: Vec::new(),
            }],
            to_create: 1,
            to_update: 0,
            to_delete: 0,
            unchanged: 0,
        };
        assert!(!plan.render().contains('\x1b'));
        let colored = plan.render_colored();
        assert!(colored.contains("\x1b[32mcreate\x1b[0m"));
        // Stripping the codes must give back the plain text.
        let stripped = colored.replace("\x1b[32m", "").replace("\x1b[0m", "");
        assert_eq!(stripped, plan.render());
    }

    #[test]
    fn plan_marks_matching_resources_noop() {
        let res = Resource {
            id: "aws_s3_bucket.b".into(),
            type_name: "aws_s3_bucket".into(),
            name: "b".into(),
            attributes: attrs(&[("acl", Value::String("private".into()))]),
        };
        let doc = Document {
            ir_version: tpt_bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![res],
            modules: vec![],
            outputs: vec![],
        };
        let prior = State::from_applied(vec![AppliedResource {
            id: "aws_s3_bucket.b".into(),
            type_name: "aws_s3_bucket".into(),
            attributes: attrs(&[("acl", Value::String("private".into()))]),
        }]);
        let order = vec!["aws_s3_bucket.b".to_string()];
        let plan = Plan::from_order(&doc, &order, &prior).unwrap();
        assert_eq!(plan.actions[0].kind, ActionKind::NoOp);
        assert_eq!(plan.unchanged, 1);
        assert!(plan.render().contains("no-op aws_s3_bucket.b"));
    }
}
