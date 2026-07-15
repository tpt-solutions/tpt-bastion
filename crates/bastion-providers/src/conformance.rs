// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider conformance suite.
//!
//! A reusable, provider-agnostic set of checks that any
//! [`ProviderPlugin`](crate::plugin::ProviderPlugin) should satisfy before it is
//! shipped. The CLI and release pipeline can run [`check`] over every registered
//! provider; [`assert_conforms`] makes it a one-line assertion inside a test.

use crate::error::ProviderError;
use crate::plugin::{ProviderPlugin, ResourceRequest};
use crate::schema::{ResourceTypeSpec, ValueType};
use bastion_ir::Value;
use std::collections::BTreeMap;

/// How serious a conformance finding is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    /// The provider is broken for this case (e.g. returns the wrong id).
    Error,
    /// The provider works but is missing a recommended capability.
    Warning,
}

/// A single conformance finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceIssue {
    /// Severity.
    pub severity: Severity,
    /// Human-readable description.
    pub message: String,
}

/// Run every conformance check against `plugin`, returning all findings (empty =
/// fully conforms).
pub fn check(plugin: &dyn ProviderPlugin) -> Vec<ConformanceIssue> {
    let mut issues = Vec::new();
    let schema = plugin.schema();

    if schema.name != plugin.name() {
        issues.push(issue(
            Severity::Error,
            format!(
                "schema name `{}` does not match plugin name `{}`",
                schema.name,
                plugin.name()
            ),
        ));
    }
    if schema.version != plugin.version() {
        issues.push(issue(
            Severity::Error,
            format!(
                "schema version `{}` does not match plugin version `{}`",
                schema.version,
                plugin.version()
            ),
        ));
    }

    for rts in &schema.resources {
        conform_resource(plugin, rts, &mut issues);
    }

    issues
}

fn conform_resource(
    plugin: &dyn ProviderPlugin,
    rts: &ResourceTypeSpec,
    issues: &mut Vec<ConformanceIssue>,
) {
    if rts.attributes.is_empty() {
        issues.push(issue(
            Severity::Warning,
            format!("resource type `{}` declares no attributes", rts.type_name),
        ));
    }
    if !rts.lifecycle.create {
        issues.push(issue(
            Severity::Warning,
            format!("resource type `{}` cannot be created", rts.type_name),
        ));
    }

    // Exercise create/delete with a representative request built from the schema.
    if rts.lifecycle.create {
        let req = sample_request(rts);
        match plugin.create(req.clone()) {
            Ok(applied) => {
                if applied.id != req.id {
                    issues.push(issue(
                        Severity::Error,
                        format!(
                            "create for `{}` returned id `{}`, expected `{}`",
                            rts.type_name, applied.id, req.id
                        ),
                    ));
                }
                if applied.type_name != req.type_name {
                    issues.push(issue(
                        Severity::Error,
                        format!(
                            "create for `{}` returned type `{}`",
                            rts.type_name, applied.type_name
                        ),
                    ));
                }
            }
            Err(e) => issues.push(issue(
                Severity::Error,
                format!("create for `{}` failed: {e}", rts.type_name),
            )),
        }

        if let Err(e) = plugin.delete(req) {
            issues.push(issue(
                Severity::Error,
                format!("delete for `{}` failed: {e}", rts.type_name),
            ));
        }
    }

    if rts.lifecycle.read {
        let req = sample_request(rts);
        if let Err(ProviderError::UnsupportedOp { .. }) = plugin.read(req) {
            issues.push(issue(
                Severity::Warning,
                format!(
                    "resource type `{}` advertises read but plugin returns UnsupportedOp",
                    rts.type_name
                ),
            ));
        }
    }
}

/// Build a representative request for a resource type, filling each declared
/// attribute with a type-appropriate placeholder.
fn sample_request(rts: &ResourceTypeSpec) -> ResourceRequest {
    let mut attributes = BTreeMap::new();
    for attr in &rts.attributes {
        if attr.computed {
            continue; // computed attrs are produced by create.
        }
        attributes.insert(attr.name.clone(), placeholder(attr.value_type));
    }
    ResourceRequest {
        id: format!("conformance.{}.sample", rts.type_name),
        type_name: rts.type_name.clone(),
        attributes,
    }
}

fn placeholder(t: ValueType) -> Value {
    match t {
        ValueType::String => Value::String("sample".into()),
        ValueType::Number => Value::Number(1.0),
        ValueType::Bool => Value::Bool(true),
        ValueType::List => Value::List(vec![]),
        ValueType::Map => Value::Map(Default::default()),
        ValueType::Any => Value::Null,
    }
}

fn issue(severity: Severity, message: impl Into<String>) -> ConformanceIssue {
    ConformanceIssue {
        severity,
        message: message.into(),
    }
}

/// Assert `plugin` fully conforms; panics (via `assert!`) on any Error-level
/// finding, making it drop-in for a test body.
pub fn assert_conforms(plugin: &dyn ProviderPlugin) {
    let issues = check(plugin);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "provider `{}` failed conformance:\n{}",
        plugin.name(),
        errors
            .iter()
            .map(|i| format!("  ERROR: {}", i.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
