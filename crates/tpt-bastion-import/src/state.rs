// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Import a Terraform state file (format version 4) as Bastion resources.
//!
//! Only managed resources with exactly one instance are imported. A resource with
//! several instances (from `count` or `for_each`) would need an indexed id that
//! Bastion does not model yet, so it is reported and skipped. Data sources are
//! read-only and are not part of the managed state.

use crate::convert::Note;
use serde_json::Value as Json;
use std::collections::BTreeMap;
use tpt_bastion_engine::AppliedResource;
use tpt_bastion_ir::Value;

/// The result of importing a Terraform state file.
#[derive(Debug, Clone, PartialEq)]
pub struct StateImport {
    /// Resources ready to be written as Bastion state.
    pub resources: Vec<AppliedResource>,
    /// Everything that was not imported, in file order.
    pub notes: Vec<Note>,
}

/// Why a state file could not be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateImportError {
    /// The file is not valid JSON.
    Json(String),
    /// The file is JSON but not a Terraform state file this importer understands.
    Unsupported(String),
}

impl std::fmt::Display for StateImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateImportError::Json(e) => write!(f, "not valid JSON: {e}"),
            StateImportError::Unsupported(e) => write!(f, "unsupported Terraform state: {e}"),
        }
    }
}

impl std::error::Error for StateImportError {}

/// Import a Terraform state file given as JSON text.
pub fn import_tfstate(text: &str) -> Result<StateImport, StateImportError> {
    let doc: Json =
        serde_json::from_str(text).map_err(|e| StateImportError::Json(e.to_string()))?;
    match doc.get("version").and_then(Json::as_u64) {
        Some(4) => {}
        Some(other) => {
            return Err(StateImportError::Unsupported(format!(
                "state format version {other}; only version 4 is supported"
            )))
        }
        None => {
            return Err(StateImportError::Unsupported(
                "no `version` field; this is not a Terraform state file".into(),
            ))
        }
    }

    let mut import = StateImport {
        resources: Vec::new(),
        notes: Vec::new(),
    };
    let resources = doc
        .get("resources")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();

    for (index, resource) in resources.iter().enumerate() {
        let mode = resource
            .get("mode")
            .and_then(Json::as_str)
            .unwrap_or("managed");
        let ty = resource.get("type").and_then(Json::as_str).unwrap_or("?");
        let name = resource.get("name").and_then(Json::as_str).unwrap_or("?");
        let label = format!("{ty}.{name}");
        // The state file has no line numbers; the resource index stands in for them.
        let line = index + 1;

        if mode == "data" {
            import.notes.push(Note {
                line,
                message: format!("data source `{label}` is read-only and is not imported"),
            });
            continue;
        }

        let instances = resource
            .get("instances")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        match instances.as_slice() {
            [instance] => {
                let attributes = instance
                    .get("attributes")
                    .and_then(Json::as_object)
                    .map(|obj| {
                        obj.iter()
                            .map(|(k, v)| (k.clone(), from_json(v)))
                            .collect::<BTreeMap<String, Value>>()
                    })
                    .unwrap_or_default();
                import.resources.push(AppliedResource {
                    id: label,
                    type_name: ty.to_string(),
                    attributes,
                });
            }
            [] => import.notes.push(Note {
                line,
                message: format!("`{label}` has no instances; not imported"),
            }),
            many => import.notes.push(Note {
                line,
                message: format!(
                    "`{label}` has {} instances (count or for_each); indexed resources are not imported yet",
                    many.len()
                ),
            }),
        }
    }
    Ok(import)
}

/// Convert a JSON value from the state file into an IR value.
fn from_json(v: &Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        Json::String(s) => Value::String(s.clone()),
        Json::Array(items) => Value::List(items.iter().map(from_json).collect()),
        Json::Object(map) => {
            Value::Map(map.iter().map(|(k, v)| (k.clone(), from_json(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{
        "version": 4,
        "resources": [
            {"mode": "managed", "type": "aws_s3_bucket", "name": "b",
             "instances": [{"attributes": {"bucket": "x", "versioning": [{"enabled": true}], "id": "x"}}]},
            {"mode": "managed", "type": "aws_s3_bucket", "name": "many",
             "instances": [{"index_key": 0, "attributes": {}}, {"index_key": 1, "attributes": {}}]},
            {"mode": "data", "type": "aws_caller_identity", "name": "me",
             "instances": [{"attributes": {}}]}
        ]
    }"#;

    #[test]
    fn imports_single_instances_and_reports_the_rest() {
        let import = import_tfstate(STATE).unwrap();
        assert_eq!(import.resources.len(), 1);
        let b = &import.resources[0];
        assert_eq!(b.id, "aws_s3_bucket.b");
        assert_eq!(b.type_name, "aws_s3_bucket");
        assert_eq!(b.attributes.get("bucket"), Some(&Value::String("x".into())));
        assert!(matches!(
            b.attributes.get("versioning"),
            Some(Value::List(_))
        ));
        assert_eq!(import.notes.len(), 2);
        assert!(import.notes[0].message.contains("2 instances"));
        assert!(import.notes[1].message.contains("data source"));
    }

    #[test]
    fn rejects_other_versions_and_non_state_json() {
        assert!(matches!(
            import_tfstate(r#"{"version": 3, "resources": []}"#),
            Err(StateImportError::Unsupported(_))
        ));
        assert!(matches!(
            import_tfstate(r#"{"resources": []}"#),
            Err(StateImportError::Unsupported(_))
        ));
        assert!(matches!(
            import_tfstate("not json"),
            Err(StateImportError::Json(_))
        ));
    }
}
