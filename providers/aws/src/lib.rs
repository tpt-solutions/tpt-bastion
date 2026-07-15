// SPDX-License-Identifier: Apache-2.0 OR MIT

//! AWS provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! This crate is the **initial** AWS provider. It implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with the
//! resource schemas Bastion needs (`aws_s3_bucket`, `aws_dynamodb_table`) and a
//! working `create`/`delete` that records applied state and computes output
//! attributes (`id`, `arn`). The real cloud calls are stubbed behind clearly
//! marked `// TODO(sdk)` points: swap the echo for `aws-sdk-s3` /
//! `aws-sdk-dynamodb` (already a workspace-friendly choice) and read credentials
//! from the standard AWS chain. The schema, lifecycle, and engine integration
//! are already correct, so the SDK swap is mechanical.
//!
//! The provider is loaded like any other: register it with a
//! [`NativeRuntime`](bastion_providers::plugin::NativeRuntime) or ship it as a
//! subprocess plugin via the provider ABI.

use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use bastion_providers::error::ProviderError;
use bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};
use std::collections::BTreeMap;

/// The AWS provider.
pub struct AwsProvider;

impl AwsProvider {
    /// Construct the AWS provider.
    pub fn new() -> Self {
        AwsProvider
    }

    fn s3_arn(bucket: &str) -> String {
        format!("arn:aws:s3:::{bucket}")
    }
}

impl Default for AwsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for AwsProvider {
    fn name(&self) -> &str {
        "aws"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("aws", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("aws_s3_bucket")
                    .description("An S3 bucket.")
                    .attribute(AttributeSpec::required("bucket", ValueType::String))
                    .attribute(AttributeSpec::optional("acl", ValueType::String))
                    .attribute(AttributeSpec::optional("region", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .attribute(AttributeSpec::computed("arn", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("aws_dynamodb_table")
                    .description("A DynamoDB table.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::required("hash_key", ValueType::String))
                    .attribute(AttributeSpec::optional("billing_mode", ValueType::String))
                    .attribute(AttributeSpec::computed("arn", ValueType::String))
                    .build(),
            )
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        let mut attributes: BTreeMap<String, Value> = req.attributes.clone();
        match req.type_name.as_str() {
            "aws_s3_bucket" => {
                let bucket = match req.attributes.get("bucket") {
                    Some(Value::String(b)) => b.clone(),
                    _ => {
                        return Err(ProviderError::Rejected {
                            provider: "aws".into(),
                            message: "aws_s3_bucket requires a string `bucket`".into(),
                        })
                    }
                };
                // TODO(sdk): call aws-sdk-s3::Client::create_bucket here.
                attributes.insert("id".to_string(), Value::String(bucket.clone()));
                attributes.insert("arn".to_string(), Value::String(Self::s3_arn(&bucket)));
            }
            "aws_dynamodb_table" => {
                let name = match req.attributes.get("name") {
                    Some(Value::String(n)) => n.clone(),
                    _ => {
                        return Err(ProviderError::Rejected {
                            provider: "aws".into(),
                            message: "aws_dynamodb_table requires a string `name`".into(),
                        })
                    }
                };
                // TODO(sdk): call aws-sdk-dynamodb::Client::create_table here.
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!("arn:aws:dynamodb:::table/{name}")),
                );
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "aws".into(),
                    type_name: other.to_string(),
                })
            }
        }
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes,
        })
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError> {
        match req.type_name.as_str() {
            "aws_s3_bucket" => {
                // TODO(sdk): call aws-sdk-s3::Client::delete_bucket here.
            }
            "aws_dynamodb_table" => {
                // TODO(sdk): call aws-sdk-dynamodb::Client::delete_table here.
            }
            other => {
                return Err(ProviderError::UnknownResourceType {
                    provider: "aws".into(),
                    type_name: other.to_string(),
                })
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(type_name: &str, bucket: &str) -> ResourceRequest {
        let mut attributes = BTreeMap::new();
        attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));
        ResourceRequest {
            id: format!("{type_name}.demo"),
            type_name: type_name.to_string(),
            attributes,
        }
    }

    #[test]
    fn creates_s3_with_computed_outputs() {
        let p = AwsProvider::new();
        let applied = p.create(req("aws_s3_bucket", "my-bucket")).unwrap();
        assert_eq!(applied.attributes.get("arn"), Some(&Value::String("arn:aws:s3:::my-bucket".into())));
        assert!(p.schema().resource("aws_s3_bucket").is_some());
    }

    #[test]
    fn rejects_unknown_type() {
        let p = AwsProvider::new();
        let r = req("aws_lambda_function", "fn");
        assert!(p.create(r).is_err());
    }
}
