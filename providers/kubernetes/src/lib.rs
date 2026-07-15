// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kubernetes provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with the
//! `kubernetes_namespace` resource type and a working `create`/`delete` that
//! records applied state and computes `id`. Real calls are stubbed behind
//! `// TODO(sdk)` points — swap the echo for `kube`/`k8s-openapi` and load the
//! kubeconfig / in-cluster config. The schema and engine integration are
//! already correct.

use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use bastion_providers::error::ProviderError;
use bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};
use std::collections::BTreeMap;

/// The Kubernetes provider.
pub struct KubernetesProvider;

impl KubernetesProvider {
    /// Construct the Kubernetes provider.
    pub fn new() -> Self {
        KubernetesProvider
    }
}

impl Default for KubernetesProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for KubernetesProvider {
    fn name(&self) -> &str {
        "kubernetes"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("kubernetes", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("kubernetes_namespace")
                    .description("A Kubernetes namespace.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
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
        let name = match req.attributes.get("metadata_name") {
            Some(Value::String(n)) => n.clone(),
            _ => {
                return Err(ProviderError::Rejected {
                    provider: "kubernetes".into(),
                    message: "kubernetes_namespace requires a string `metadata_name`".into(),
                })
            }
        };
        // TODO(sdk): call kube client here.
        attributes.insert("id".to_string(), Value::String(name.clone()));
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {
        // TODO(sdk): call kube client delete namespace here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_namespace_with_id() {
        let p = KubernetesProvider::new();
        let mut attributes = BTreeMap::new();
        attributes.insert("metadata_name".to_string(), Value::String("team-a".into()));
        let req = ResourceRequest {
            id: "kubernetes_namespace.demo".into(),
            type_name: "kubernetes_namespace".into(),
            attributes,
        };
        let applied = p.create(req).unwrap();
        assert_eq!(
            applied.attributes.get("id"),
            Some(&Value::String("team-a".into()))
        );
    }
}
