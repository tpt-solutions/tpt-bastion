// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # tpt-bastion-ir
//!
//! The intermediate representation (IR) for [TPT Bastion][bastion], the
//! next-generation Infrastructure-as-Code platform.
//!
//! The Bastion DSL compiles down to this IR, which is intentionally stable
//! and versioned. Providers, the execution engine, and the state backends all
//! consume IR rather than the source DSL, which keeps the toolchain
//! decoupled and lets language bindings (TypeScript, Python) target the same
//! artifact.
//!
//! ## IR versioning
//!
//! [`IR_VERSION`] is bumped only when the on-disk/over-the-wire shape of
//! [`Document`] changes in a backwards-incompatible way. Producers embed the
//! version they emitted; consumers reject documents with a newer major so that
//! old engines fail loudly instead of misinterpreting data.
//!
//! [bastion]: https://bastion.tpt.dev

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The IR format version emitted by this crate.
///
/// Encoded into [`Document::ir_version`] on every compile so consumers can
/// gate on compatibility.
pub const IR_VERSION: u32 = 1;

/// A fully-compiled Bastion document.
///
/// This is the artifact produced by the DSL compiler and consumed by the
/// execution engine, state backends, and provider runtime. It is
/// deterministic: a given source document always produces the same IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// IR format version this document was produced with.
    pub ir_version: u32,

    /// Optional top-level `config { ... }` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Config>,

    /// Declared input variables and their defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,

    /// Managed resources, keyed by their `type.name` id for stable ordering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<Resource>,

    /// Module instantiations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<Module>,

    /// Output values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Output>,
}

/// Top-level configuration (project metadata, default region, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// Any extra metadata declared in the config block.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// A declared input variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// A managed resource: a single unit of infrastructure (a bucket, a VM, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Fully-qualified id, e.g. `aws_s3_bucket.my_bucket`.
    pub id: String,

    /// Provider-scoped type, e.g. `aws_s3_bucket`.
    pub type_name: String,

    /// Local name within the document, e.g. `my_bucket`.
    pub name: String,

    /// Declared attributes. References remain unevaluated until apply time.
    pub attributes: BTreeMap<String, Value>,
}

/// A module instantiation. `source` points at the module definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// Inputs passed to the module.
    pub attributes: BTreeMap<String, Value>,
}

/// A named output value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub name: String,
    pub value: Value,
}

/// A compile-time IR value.
///
/// Reference values are preserved as structured pointers so the execution
/// engine can resolve them against the planned graph rather than baking in a
/// literal. This is what enables compile-time reference validation and
/// dependency-graph construction downstream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Value {
    /// A UTF-8 string literal.
    String(String),

    /// A numeric literal (integers and floats share one representation).
    Number(f64),

    /// A boolean literal.
    Bool(bool),

    /// The explicit `null` literal.
    Null,

    /// A reference to another object's attribute, e.g. `my_bucket.bucket`.
    ///
    /// `path` is the attribute chain after the base identifier.
    Reference {
        /// Base identifier (resource/module/variable name).
        base: String,
        /// Optional attribute access, e.g. `["bucket"]` for `my_bucket.bucket`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        path: Vec<String>,
    },

    /// An ordered list.
    List(Vec<Value>),

    /// An ordered map of string keys to values.
    Map(BTreeMap<String, Value>),
}

impl Value {
    /// Returns `true` if this value (or any nested value) contains a reference.
    pub fn contains_reference(&self) -> bool {
        match self {
            Value::Reference { .. } => true,
            Value::List(items) => items.iter().any(Value::contains_reference),
            Value::Map(map) => map.values().any(Value::contains_reference),
            _ => false,
        }
    }
}

/// Construct an empty document with the current [`IR_VERSION`].
pub fn empty_document() -> Document {
    Document {
        ir_version: IR_VERSION,
        config: None,
        variables: Vec::new(),
        resources: Vec::new(),
        modules: Vec::new(),
        outputs: Vec::new(),
    }
}

/// Returns `true` if a document's IR version is consumable by this crate.
///
/// A newer major is incompatible; older majors are accepted (the engine
/// performs any necessary migrations).
pub fn is_compatible(ir_version: u32) -> bool {
    ir_version <= IR_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_recorded_and_compatible() {
        let doc = empty_document();
        assert_eq!(doc.ir_version, IR_VERSION);
        assert!(is_compatible(IR_VERSION));
        assert!(is_compatible(0));
        assert!(!is_compatible(IR_VERSION + 1));
    }

    #[test]
    fn reference_shape_round_trips() {
        let value = Value::Reference {
            base: "my_bucket".into(),
            path: vec!["bucket".into()],
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn document_serializes_stable_keys() {
        let doc = Document {
            ir_version: IR_VERSION,
            config: Some(Config {
                name: Some("demo".into()),
                region: None,
                extra: BTreeMap::new(),
            }),
            variables: vec![],
            resources: vec![Resource {
                id: "aws_s3_bucket.demo".into(),
                type_name: "aws_s3_bucket".into(),
                name: "demo".into(),
                attributes: {
                    let mut m = BTreeMap::new();
                    m.insert("bucket".into(), Value::String("demo-bucket".into()));
                    m
                },
            }],
            modules: vec![],
            outputs: vec![],
        };
        let json = serde_json::to_string_pretty(&doc).unwrap();
        assert!(json.contains("aws_s3_bucket.demo"));
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }
}
