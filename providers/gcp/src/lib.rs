// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Google Cloud provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with a
//! broad set of core resource schemas (`google_storage_bucket`,
//! `google_compute_instance`, `google_compute_network`, `google_pubsub_topic`,
//! `google_sql_database_instance`, `google_container_cluster`) and a working
//! `create`/`delete` that records applied state and computes `id`/`self_link`.
//! Real calls are stubbed behind `// TODO(sdk)` points — swap the echo for the
//! relevant `google-cloud-*` crate and read credentials from the standard GCP
//! application-default chain. The schema and engine integration are already
//! correct.

use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use bastion_providers::error::ProviderError;
use bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};
use std::collections::BTreeMap;

/// The Google Cloud provider.
pub struct GcpProvider;

impl GcpProvider {
    /// Construct the GCP provider.
    pub fn new() -> Self {
        GcpProvider
    }
}

impl Default for GcpProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for GcpProvider {
    fn name(&self) -> &str {
        "google"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("google", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("google_storage_bucket")
                    .description("A Google Cloud Storage bucket.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("location", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("url", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("google_compute_instance")
                    .description("A Compute Engine VM instance.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required("machine_type", ValueType::String))
                    .attribute(AttributeSpec::optional("zone", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("self_link", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("google_compute_network")
                    .description("A Compute Engine VPC network.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("routing_mode", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("self_link", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("google_pubsub_topic")
                    .description("A Pub/Sub topic.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("name_out", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("google_sql_database_instance")
                    .description("A Cloud SQL database instance.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("database_version", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("self_link", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("google_container_cluster")
                    .description("A GKE Kubernetes cluster.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required("location", ValueType::String))
                    .attribute(AttributeSpec::optional("node_count", ValueType::Number))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("endpoint", ValueType::String))
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
            "google_storage_bucket" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-storage client here.
                attributes.insert("url".to_string(), Value::String(format!("gs://{name}")));
                name
            }
            "google_compute_instance" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-compute instances().insert here.
                attributes.insert(
                    "self_link".to_string(),
                    Value::String(format!(
                        "https://www.googleapis.com/compute/v1/projects/demo/zones/us-central1-a/instances/{name}"
                    )),
                );
                name
            }
            "google_compute_network" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-compute networks().insert here.
                attributes.insert(
                    "self_link".to_string(),
                    Value::String(format!(
                        "https://www.googleapis.com/compute/v1/projects/demo/global/networks/{name}"
                    )),
                );
                name
            }
            "google_pubsub_topic" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-pubsub topic admin here.
                attributes.insert("name_out".to_string(), Value::String(name.clone()));
                name
            }
            "google_sql_database_instance" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-sql admin here.
                attributes.insert(
                    "self_link".to_string(),
                    Value::String(format!(
                        "https://sqladmin.googleapis.com/v1/projects/demo/instances/{name}"
                    )),
                );
                name
            }
            "google_container_cluster" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call google-cloud-container clusters().create here.
                attributes.insert(
                    "endpoint".to_string(),
                    Value::String(format!("https://{name}.container.googleapis.com")),
                );
                name
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "google".into(),
                    type_name: other.to_string(),
                })
            }
        };
        attributes.insert("id".to_string(), Value::String(id.clone()));
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError> {
        match req.type_name.as_str() {
            "google_storage_bucket" => {
                // TODO(sdk): call google-cloud-storage client delete_bucket here.
            }
            "google_compute_instance" => {
                // TODO(sdk): call compute instances().delete here.
            }
            "google_compute_network" => {
                // TODO(sdk): call compute networks().delete here.
            }
            "google_pubsub_topic" => {
                // TODO(sdk): call pubsub topic admin delete_topic here.
            }
            "google_sql_database_instance" => {
                // TODO(sdk): call sql admin instances().delete here.
            }
            "google_container_cluster" => {
                // TODO(sdk): call container clusters().delete here.
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "google".into(),
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
            provider: "google".into(),
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
        assert_conforms(&GcpProvider::new());
    }

    #[test]
    fn creates_bucket_with_url() {
        let p = GcpProvider::new();
        let mut attributes = BTreeMap::new();
        attributes.insert("name".to_string(), Value::String("my-bucket".into()));
        let req = ResourceRequest {
            id: "google_storage_bucket.demo".into(),
            type_name: "google_storage_bucket".into(),
            attributes,
        };
        let applied = p.create(req).unwrap();
        assert_eq!(
            applied.attributes.get("url"),
            Some(&Value::String("gs://my-bucket".into()))
        );
    }
}
