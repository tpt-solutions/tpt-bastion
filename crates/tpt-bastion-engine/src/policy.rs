// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Plan-time policy evaluation.
//!
//! A [`Policy`] states that an attribute of resources of one type must not equal,
//! or must equal, a value. [`evaluate`] checks every declared resource against
//! every policy. Planning fails if any rule is violated, so a violating document
//! never reaches apply.
//!
//! A reference-valued attribute is not checked. Its value is only known after
//! apply, and a check now could pass or fail for the wrong reason.

use crate::plan::display_value;
use tpt_bastion_ir::{Document, Policy, PolicyRule};

/// One rule that a document breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The policy that was broken.
    pub policy: String,
    /// The resource that breaks it.
    pub resource: String,
    /// Why, from the policy's `message` or a default.
    pub message: String,
}

/// Check every declared resource against every policy in `doc`.
pub fn evaluate(doc: &Document) -> Vec<Violation> {
    let mut violations = Vec::new();
    for policy in &doc.policies {
        for resource in doc
            .resources
            .iter()
            .filter(|r| r.type_name == policy.resource_type)
        {
            if let Some(message) = check(policy, resource.attributes.get(&policy.attribute)) {
                violations.push(Violation {
                    policy: policy.name.clone(),
                    resource: resource.id.clone(),
                    message,
                });
            }
        }
    }
    violations
}

/// The reason `actual` breaks `policy`, or `None` if it complies or cannot be checked.
fn check(policy: &Policy, actual: Option<&tpt_bastion_ir::Value>) -> Option<String> {
    let explain = |default: String| policy.message.clone().unwrap_or(default);
    match (&policy.rule, actual) {
        (PolicyRule::Require { value }, None) => Some(explain(format!(
            "`{}` is required to be {}",
            policy.attribute,
            display_value(value)
        ))),
        (_, Some(actual)) if actual.contains_reference() => None,
        (PolicyRule::Forbid { value }, Some(actual)) if actual == value => Some(explain(format!(
            "`{}` must not be {}",
            policy.attribute,
            display_value(value)
        ))),
        (PolicyRule::Require { value }, Some(actual)) if actual != value => Some(explain(format!(
            "`{}` must be {}, not {}",
            policy.attribute,
            display_value(value),
            display_value(actual)
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tpt_bastion_ir::{Resource, Value, IR_VERSION};

    fn bucket(acl: &str) -> Resource {
        let mut attributes = BTreeMap::new();
        attributes.insert("acl".to_string(), Value::String(acl.into()));
        Resource {
            id: "aws_s3_bucket.b".into(),
            type_name: "aws_s3_bucket".into(),
            name: "b".into(),
            attributes,
        }
    }

    fn doc(resource: Resource, policies: Vec<Policy>) -> Document {
        Document {
            ir_version: IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![resource],
            modules: vec![],
            outputs: vec![],
            policies,
        }
    }

    fn forbid_public() -> Policy {
        Policy {
            name: "no-public-buckets".into(),
            resource_type: "aws_s3_bucket".into(),
            attribute: "acl".into(),
            rule: PolicyRule::Forbid {
                value: Value::String("public-read".into()),
            },
            message: Some("public buckets leak data".into()),
        }
    }

    #[test]
    fn forbidden_value_is_a_violation_with_the_policy_message() {
        let violations = evaluate(&doc(bucket("public-read"), vec![forbid_public()]));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].resource, "aws_s3_bucket.b");
        assert_eq!(violations[0].message, "public buckets leak data");
    }

    #[test]
    fn compliant_and_other_type_resources_pass() {
        assert!(evaluate(&doc(bucket("private"), vec![forbid_public()])).is_empty());
        let mut other = forbid_public();
        other.resource_type = "aws_ec2_instance".into();
        assert!(evaluate(&doc(bucket("public-read"), vec![other])).is_empty());
    }

    #[test]
    fn require_rule_flags_a_different_or_missing_value() {
        let require = Policy {
            rule: PolicyRule::Require {
                value: Value::String("private".into()),
            },
            message: None,
            ..forbid_public()
        };
        let different = evaluate(&doc(bucket("public-read"), vec![require.clone()]));
        assert_eq!(different.len(), 1);
        assert!(different[0].message.contains("must be \"private\""));

        let mut missing = bucket("x");
        missing.attributes.clear();
        assert_eq!(evaluate(&doc(missing, vec![require])).len(), 1);
    }

    #[test]
    fn reference_values_are_not_checked() {
        let mut resource = bucket("x");
        resource.attributes.insert(
            "acl".into(),
            Value::Reference {
                base: "other".into(),
                path: vec!["acl".into()],
            },
        );
        assert!(evaluate(&doc(resource, vec![forbid_public()])).is_empty());
    }
}
