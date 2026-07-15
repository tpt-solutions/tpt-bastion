// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A built-in reference provider (`null`) used by the CLI's default registry and
//! as a minimal, fully-working example of the [`ProviderPlugin`](crate::plugin::ProviderPlugin)
//! SDK. It echoes declared attributes and computes a synthetic `id`; it performs
//! no real API calls.

use crate::error::ProviderError;
use crate::plugin::{ProviderPlugin, ResourceRequest};
use crate::schema::{AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType};
use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use std::collections::BTreeMap;

/// A no-op provider that records nothing and touches no real infrastructure.
pub struct NullProvider;

impl NullProvider {
    /// Construct the built-in null provider.
    pub fn new() -> Self {
        NullProvider
    }
}

impl Default for NullProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for NullProvider {
    fn name(&self) -> &str {
        "null"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("null", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("null_resource")
                    .description(
                        "A no-op resource managed by the null provider (echoes attributes).",
                    )
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: false,
                        update: false,
                        delete: true,
                    })
                    .build(),
            )
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        let mut attributes: BTreeMap<String, Value> = req.attributes.clone();
        attributes.insert("id".to_string(), Value::String(format!("null::{}", req.id)));
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {
        Ok(())
    }
}
