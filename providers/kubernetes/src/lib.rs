// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kubernetes provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with a
//! broad set of core resource schemas (`kubernetes_namespace`,
//! `kubernetes_deployment`, `kubernetes_service`, `kubernetes_config_map`,
//! `kubernetes_secret`, `kubernetes_ingress`) and a working `create`/`delete`
//! that records applied state and computes `id`. Real calls are stubbed behind
//! `// TODO(sdk)` points — swap the echo for `kube`/`k8s-openapi` and load
//! the kubeconfig / in-cluster config. The schema and engine integration are
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
            .resource(
                ResourceTypeSpec::builder("kubernetes_deployment")
                    .description("A Kubernetes Deployment.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
                    .attribute(AttributeSpec::optional("replicas", ValueType::Number))
                    .attribute(AttributeSpec::optional("image", ValueType::String))
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
                ResourceTypeSpec::builder("kubernetes_service")
                    .description("A Kubernetes Service.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
                    .attribute(AttributeSpec::optional("port", ValueType::Number))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("cluster_ip", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("kubernetes_config_map")
                    .description("A Kubernetes ConfigMap.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
                    .attribute(AttributeSpec::optional("data", ValueType::Map))
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
                ResourceTypeSpec::builder("kubernetes_secret")
                    .description("A Kubernetes Secret.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
                    .attribute(AttributeSpec::optional("type", ValueType::String))
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
                ResourceTypeSpec::builder("kubernetes_ingress")
                    .description("A Kubernetes Ingress.")
                    .attribute(AttributeSpec::required("metadata_name", ValueType::String))
                    .attribute(AttributeSpec::optional("class_name", ValueType::String))
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
            "kubernetes_namespace" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client here.
                name
            }
            "kubernetes_deployment" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client create Deployment here.
                name
            }
            "kubernetes_service" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client create Service here.
                attributes.insert("cluster_ip".to_string(), Value::String("10.0.0.1".into()));
                name
            }
            "kubernetes_config_map" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client create ConfigMap here.
                name
            }
            "kubernetes_secret" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client create Secret here.
                name
            }
            "kubernetes_ingress" => {
                let name = require_string(&req, "metadata_name")?;
                // TODO(sdk): call kube client create Ingress here.
                name
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "kubernetes".into(),
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
            "kubernetes_namespace" => {
                // TODO(sdk): call kube client delete namespace here.
            }
            "kubernetes_deployment" => {
                // TODO(sdk): call kube client delete Deployment here.
            }
            "kubernetes_service" => {
                // TODO(sdk): call kube client delete Service here.
            }
            "kubernetes_config_map" => {
                // TODO(sdk): call kube client delete ConfigMap here.
            }
            "kubernetes_secret" => {
                // TODO(sdk): call kube client delete Secret here.
            }
            "kubernetes_ingress" => {
                // TODO(sdk): call kube client delete Ingress here.
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "kubernetes".into(),
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
            provider: "kubernetes".into(),
            message: format!("`{key}` is required and must be a string"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bastion_providers::conformance::assert_conforms;

    #[test]
    fn conforms_to_sdk() {
        assert_conforms(&KubernetesProvider::new());
    }

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

    #[test]
    fn computes_service_cluster_ip() {
        let p = KubernetesProvider::new();
        let mut attributes = BTreeMap::new();
        attributes.insert("metadata_name".to_string(), Value::String("web".into()));
        let req = ResourceRequest {
            id: "kubernetes_service.demo".into(),
            type_name: "kubernetes_service".into(),
            attributes,
        };
        let applied = p.create(req).unwrap();
        assert_eq!(
            applied.attributes.get("cluster_ip"),
            Some(&Value::String("10.0.0.1".into()))
        );
    }
}
