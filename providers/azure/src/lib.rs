// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Azure provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with the
//! `azurerm_resource_group` resource type and a working `create`/`delete` that
//! records applied state and computes `id`. Real calls are stubbed behind
//! `// TODO(sdk)` points — swap the echo for the Azure SDK (`azure_mgmt_*`) and
//! authenticate via `DefaultAzureCredential`. The schema and engine integration
//! are already correct.

use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use bastion_providers::error::ProviderError;
use bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};
use std::collections::BTreeMap;

/// The Azure provider.
pub struct AzureProvider;

impl AzureProvider {
    /// Construct the Azure provider.
    pub fn new() -> Self {
        AzureProvider
    }
}

impl Default for AzureProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for AzureProvider {
    fn name(&self) -> &str {
        "azurerm"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("azurerm", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("azurerm_resource_group")
                    .description("An Azure resource group.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        let mut attributes: BTreeMap<String, Value> = req.attributes.clone();
        let name = match req.attributes.get("name") {
            Some(Value::String(n)) => n.clone(),
            _ => {
                return Err(ProviderError::Rejected {
                    provider: "azurerm".into(),
                    message: "azurerm_resource_group requires a string `name`".into(),
                })
            }
        };
        // TODO(sdk): call azure_mgmt_resources client here.
        attributes.insert(
            "id".to_string(),
            Value::String(format!(
                "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/{name}"
            )),
        );
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {
        // TODO(sdk): call azure_mgmt_resources client delete here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_resource_group_with_id() {
        let p = AzureProvider::new();
        let mut attributes = BTreeMap::new();
        attributes.insert("name".to_string(), Value::String("rg".into()));
        attributes.insert("location".to_string(), Value::String("eastus".into()));
        let req = ResourceRequest {
            id: "azurerm_resource_group.demo".into(),
            type_name: "azurerm_resource_group".into(),
            attributes,
        };
        let applied = p.create(req).unwrap();
        assert!(applied.attributes.get("id").is_some());
    }
}
