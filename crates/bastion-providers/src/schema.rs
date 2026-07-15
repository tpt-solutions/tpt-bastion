// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider capability / schema format.
//!
//! A [`ProviderSchema`] is the *data* description of everything a provider can
//! do: the resource types it manages, the attributes each accepts (and whether
//! they are required or computed), and which lifecycle operations are supported.
//! Tooling (the CLI, docs generators, future validation passes) can read a
//! schema without ever executing the provider.

use serde::{Deserialize, Serialize};

/// The full capability surface of a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSchema {
    /// Provider name (e.g. `aws`, `local`).
    pub name: String,
    /// Provider version (semver).
    pub version: String,
    /// Resource types the provider manages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ResourceTypeSpec>,
    /// Data sources the provider exposes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_sources: Vec<DataSourceSpec>,
}

/// A managed resource type, e.g. `aws_s3_bucket`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceTypeSpec {
    /// Provider-scoped type name.
    pub type_name: String,
    /// Human-readable description (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Attributes this type accepts or computes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeSpec>,
    /// Which lifecycle operations the provider supports for this type.
    #[serde(default)]
    pub lifecycle: LifecycleOps,
}

/// A data source the provider can query (read-only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSourceSpec {
    /// Data source name (e.g. `aws_ami`).
    pub name: String,
    /// Attributes the data source accepts as arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeSpec>,
}

/// A single attribute of a resource or data source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeSpec {
    /// Attribute name.
    pub name: String,
    /// The value type this attribute holds.
    pub value_type: ValueType,
    /// Whether the attribute must be supplied by the user.
    #[serde(default)]
    pub required: bool,
    /// Whether the provider computes the value (read-only output).
    #[serde(default)]
    pub computed: bool,
}

/// The supported lifecycle operations for a resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleOps {
    /// Can resources of this type be created?
    #[serde(default = "default_true")]
    pub create: bool,
    /// Can resources of this type be read back?
    #[serde(default = "default_true")]
    pub read: bool,
    /// Can resources of this type be updated in place?
    #[serde(default = "default_true")]
    pub update: bool,
    /// Can resources of this type be destroyed?
    #[serde(default = "default_true")]
    pub delete: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LifecycleOps {
    fn default() -> Self {
        LifecycleOps {
            create: true,
            read: true,
            update: true,
            delete: true,
        }
    }
}

/// The value types an attribute may hold, mirroring [`bastion_ir::Value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    /// A UTF-8 string.
    String,
    /// A number (integer or float).
    Number,
    /// A boolean.
    Bool,
    /// An ordered list.
    List,
    /// A string-keyed map.
    Map,
    /// Any type (provider validates).
    Any,
}

impl ProviderSchema {
    /// Start building a schema for `name` / `version`.
    pub fn builder(name: impl Into<String>, version: impl Into<String>) -> SchemaBuilder {
        SchemaBuilder {
            inner: ProviderSchema {
                name: name.into(),
                version: version.into(),
                resources: Vec::new(),
                data_sources: Vec::new(),
            },
        }
    }

    /// Find a resource type spec by name.
    pub fn resource(&self, type_name: &str) -> Option<&ResourceTypeSpec> {
        self.resources.iter().find(|r| r.type_name == type_name)
    }
}

/// Builder for [`ProviderSchema`], used by plugins to declare their surface.
pub struct SchemaBuilder {
    inner: ProviderSchema,
}

impl SchemaBuilder {
    /// Add a resource type spec.
    pub fn resource(mut self, spec: ResourceTypeSpec) -> Self {
        self.inner.resources.push(spec);
        self
    }

    /// Add a data source spec.
    pub fn data_source(mut self, spec: DataSourceSpec) -> Self {
        self.inner.data_sources.push(spec);
        self
    }

    /// Finalize the schema.
    pub fn build(self) -> ProviderSchema {
        self.inner
    }
}

impl ResourceTypeSpec {
    /// Start building a resource type spec.
    pub fn builder(type_name: impl Into<String>) -> ResourceTypeBuilder {
        ResourceTypeBuilder {
            inner: ResourceTypeSpec {
                type_name: type_name.into(),
                description: None,
                attributes: Vec::new(),
                lifecycle: LifecycleOps::default(),
            },
        }
    }
}

/// Builder for [`ResourceTypeSpec`].
pub struct ResourceTypeBuilder {
    inner: ResourceTypeSpec,
}

impl ResourceTypeBuilder {
    /// Set a human-readable description.
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.inner.description = Some(d.into());
        self
    }

    /// Add an attribute.
    pub fn attribute(mut self, spec: AttributeSpec) -> Self {
        self.inner.attributes.push(spec);
        self
    }

    /// Override the supported lifecycle ops.
    pub fn lifecycle(mut self, ops: LifecycleOps) -> Self {
        self.inner.lifecycle = ops;
        self
    }

    /// Finalize the resource type spec.
    pub fn build(self) -> ResourceTypeSpec {
        self.inner
    }
}

impl AttributeSpec {
    /// A required, user-supplied attribute of the given type.
    pub fn required(name: impl Into<String>, value_type: ValueType) -> Self {
        AttributeSpec {
            name: name.into(),
            value_type,
            required: true,
            computed: false,
        }
    }

    /// A computed (read-only output) attribute of the given type.
    pub fn computed(name: impl Into<String>, value_type: ValueType) -> Self {
        AttributeSpec {
            name: name.into(),
            value_type,
            required: false,
            computed: true,
        }
    }

    /// An optional, user-supplied attribute of the given type.
    pub fn optional(name: impl Into<String>, value_type: ValueType) -> Self {
        AttributeSpec {
            name: name.into(),
            value_type,
            required: false,
            computed: false,
        }
    }
}
