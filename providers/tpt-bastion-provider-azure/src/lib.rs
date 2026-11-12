// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Azure provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](tpt_bastion_providers::plugin::ProviderPlugin) SDK with a
//! broad set of core resource schemas (`azurerm_resource_group`,
//! `azurerm_storage_account`, `azurerm_virtual_network`, `azurerm_key_vault`,
//! `azurerm_sql_server`, `azurerm_linux_virtual_machine`) and a working
//! `create`/`delete` that records applied state and computes `id`. Real calls
//! are stubbed behind `// TODO(sdk)` points — swap the echo for the Azure SDK
//! (`azure_mgmt_*`) and authenticate via `DefaultAzureCredential`. The schema
//! and engine integration are already correct.

use std::collections::BTreeMap;
use tpt_bastion_engine::provider::AppliedResource;
use tpt_bastion_ir::Value;
use tpt_bastion_providers::error::ProviderError;
use tpt_bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use tpt_bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};

/// The Azure provider.
pub struct AzureProvider;

impl AzureProvider {
    /// Construct the Azure provider.
    pub fn new() -> Self {
        AzureProvider
    }

    fn resource_id(kind: &str, name: &str) -> String {
        format!(
            "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/{name}/providers/{kind}/{name}"
        )
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
            .resource(
                ResourceTypeSpec::builder("azurerm_storage_account")
                    .description("An Azure Storage account.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "resource_group_name",
                        ValueType::String,
                    ))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::optional("account_tier", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("azurerm_virtual_network")
                    .description("An Azure virtual network.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "resource_group_name",
                        ValueType::String,
                    ))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::required("address_space", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("azurerm_key_vault")
                    .description("An Azure Key Vault.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "resource_group_name",
                        ValueType::String,
                    ))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::required("tenant_id", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("azurerm_sql_server")
                    .description("An Azure SQL server.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "resource_group_name",
                        ValueType::String,
                    ))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "administrator_login",
                        ValueType::String,
                    ))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("azurerm_linux_virtual_machine")
                    .description("A Linux Azure virtual machine.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required(
                        "resource_group_name",
                        ValueType::String,
                    ))
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
        let id = match req.type_name.as_str() {
            "azurerm_resource_group" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_resources client here.
                format!("/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/{name}")
            }
            "azurerm_storage_account" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_storage client here.
                Self::resource_id("Microsoft.Storage/storageAccounts", &name)
            }
            "azurerm_virtual_network" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_network client here.
                Self::resource_id("Microsoft.Network/virtualNetworks", &name)
            }
            "azurerm_key_vault" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_keyvault client here.
                Self::resource_id("Microsoft.KeyVault/vaults", &name)
            }
            "azurerm_sql_server" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_sql client here.
                Self::resource_id("Microsoft.Sql/servers", &name)
            }
            "azurerm_linux_virtual_machine" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call azure_mgmt_compute client here.
                Self::resource_id("Microsoft.Compute/virtualMachines", &name)
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "azurerm".into(),
                    type_name: other.to_string(),
                })
            }
        };
        attributes.insert("id".to_string(), Value::String(id));
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError> {
        match req.type_name.as_str() {
            "azurerm_resource_group" => {
                // TODO(sdk): call azure_mgmt_resources client delete here.
            }
            "azurerm_storage_account" => {
                // TODO(sdk): call azure_mgmt_storage client delete here.
            }
            "azurerm_virtual_network" => {
                // TODO(sdk): call azure_mgmt_network client delete here.
            }
            "azurerm_key_vault" => {
                // TODO(sdk): call azure_mgmt_keyvault client delete here.
            }
            "azurerm_sql_server" => {
                // TODO(sdk): call azure_mgmt_sql client delete here.
            }
            "azurerm_linux_virtual_machine" => {
                // TODO(sdk): call azure_mgmt_compute client delete here.
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "azurerm".into(),
                    type_name: other.to_string(),
                })
            }
        }
        Ok(())
    }
}

/// Require `key` to be present and a string, returning a clear rejection otherwise.
fn require_string(req: &ResourceRequest, key: &str) -> Result<String, ProviderError> {
    match req.attributes.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(ProviderError::Rejected {
            provider: "azurerm".into(),
            message: format!("`{key}` is required and must be a string"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_bastion_providers::conformance::assert_conforms;

    #[test]
    fn conforms_to_sdk() {
        assert_conforms(&AzureProvider::new());
    }

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
        assert!(applied.attributes.contains_key("id"));
    }
}
