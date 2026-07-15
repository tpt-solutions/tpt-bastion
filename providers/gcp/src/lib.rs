// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Google Cloud provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! Initial provider: implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with the
//! `google_storage_bucket` resource type and a working `create`/`delete` that
//! records applied state and computes `id`/`url`. Real calls are stubbed behind
//! `// TODO(sdk)` points — swap the echo for `google-cloud-storage` and read
//! credentials from the standard GCP application-default chain. The schema and
//! engine integration are already correct.

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
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        let mut attributes: BTreeMap<String, Value> = req.attributes.clone();
        let name = match req.attributes.get("name") {
            Some(Value::String(n)) => n.clone(),
            _ => {
                return Err(ProviderError::Rejected {
                    provider: "google".into(),
                    message: "google_storage_bucket requires a string `name`".into(),
                })
            }
        };
        // TODO(sdk): call google-cloud-storage client here.
        attributes.insert("id".to_string(), Value::String(name.clone()));
        attributes.insert(
            "url".to_string(),
            Value::String(format!("gs://{name}")),
        );
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {
        // TODO(sdk): call google-cloud-storage client delete_bucket here.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
