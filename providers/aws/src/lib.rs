// SPDX-License-Identifier: Apache-2.0 OR MIT

//! AWS provider for TPT Bastion (Phase 6 — core resource coverage).
//!
//! This crate is the **initial** AWS provider. It implements the
//! [`ProviderPlugin`](bastion_providers::plugin::ProviderPlugin) SDK with a
//! broad set of core resource schemas (`aws_s3_bucket`, `aws_dynamodb_table`,
//! `aws_iam_role`, `aws_security_group`, `aws_vpc`, `aws_sqs_queue`,
//! `aws_lambda_function`) and working `create`/`delete` that record applied
//! state and compute output attributes (`id`, `arn`, `url`). The real cloud
//! calls are stubbed behind clearly marked `// TODO(sdk)` points: swap the echo
//! for the relevant `aws-sdk-*` crate (already a workspace-friendly choice) and
//! read credentials from the standard AWS chain. The schema, lifecycle, and
//! engine integration are already correct, so the SDK swap is mechanical.
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
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .resource(
                ResourceTypeSpec::builder("aws_iam_role")
                    .description("An IAM role.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("assume_role_policy", ValueType::String))
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
                ResourceTypeSpec::builder("aws_security_group")
                    .description("An EC2 security group.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("description", ValueType::String))
                    .attribute(AttributeSpec::optional("vpc_id", ValueType::String))
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
                ResourceTypeSpec::builder("aws_vpc")
                    .description("An EC2 VPC.")
                    .attribute(AttributeSpec::required("cidr_block", ValueType::String))
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
                ResourceTypeSpec::builder("aws_sqs_queue")
                    .description("An SQS queue.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::optional("fifo", ValueType::Bool))
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
                ResourceTypeSpec::builder("aws_lambda_function")
                    .description("A Lambda function.")
                    .attribute(AttributeSpec::required("function_name", ValueType::String))
                    .attribute(AttributeSpec::required("runtime", ValueType::String))
                    .attribute(AttributeSpec::required("handler", ValueType::String))
                    .attribute(AttributeSpec::optional("role", ValueType::String))
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
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        let mut attributes: BTreeMap<String, Value> = req.attributes.clone();
        match req.type_name.as_str() {
            "aws_s3_bucket" => {
                let bucket = require_string(&req, "bucket")?;
                // TODO(sdk): call aws-sdk-s3::Client::create_bucket here.
                attributes.insert("id".to_string(), Value::String(bucket.clone()));
                attributes.insert("arn".to_string(), Value::String(Self::s3_arn(&bucket)));
            }
            "aws_dynamodb_table" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call aws-sdk-dynamodb::Client::create_table here.
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!("arn:aws:dynamodb:::table/{name}")),
                );
            }
            "aws_iam_role" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call aws-sdk-iam::Client::create_role here.
                attributes.insert("id".to_string(), Value::String(name.clone()));
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!("arn:aws:iam:::role/{name}")),
                );
            }
            "aws_security_group" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call aws-sdk-ec2::Client::create_security_group here.
                attributes.insert("id".to_string(), Value::String(name.clone()));
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!(
                        "arn:aws:ec2:us-east-1:123456789012:security-group/{name}"
                    )),
                );
            }
            "aws_vpc" => {
                let cidr = require_string(&req, "cidr_block")?;
                let suffix: String = cidr.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                // TODO(sdk): call aws-sdk-ec2::Client::create_vpc here.
                attributes.insert("id".to_string(), Value::String(format!("vpc-{suffix}")));
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!(
                        "arn:aws:ec2:us-east-1:123456789012:vpc/vpc-{suffix}"
                    )),
                );
            }
            "aws_sqs_queue" => {
                let name = require_string(&req, "name")?;
                // TODO(sdk): call aws-sdk-sqs::Client::create_queue here.
                attributes.insert("id".to_string(), Value::String(name.clone()));
                attributes.insert(
                    "url".to_string(),
                    Value::String(format!("https://sqs.us-east-1.amazonaws.com/123456789012/{name}")),
                );
            }
            "aws_lambda_function" => {
                let name = require_string(&req, "function_name")?;
                // TODO(sdk): call aws-sdk-lambda::Client::create_function here.
                attributes.insert("id".to_string(), Value::String(name.clone()));
                attributes.insert(
                    "arn".to_string(),
                    Value::String(format!(
                        "arn:aws:lambda:us-east-1:123456789012:function:{name}"
                    )),
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
            "aws_iam_role" => {
                // TODO(sdk): call aws-sdk-iam::Client::delete_role here.
            }
            "aws_security_group" => {
                // TODO(sdk): call aws-sdk-ec2::Client::delete_security_group here.
            }
            "aws_vpc" => {
                // TODO(sdk): call aws-sdk-ec2::Client::delete_vpc here.
            }
            "aws_sqs_queue" => {
                // TODO(sdk): call aws-sdk-sqs::Client::delete_queue here.
            }
            "aws_lambda_function" => {
                // TODO(sdk): call aws-sdk-lambda::Client::delete_function here.
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

/// Require `key` to be present and a string, returning a clear rejection otherwise.
fn require_string(req: &ResourceRequest, key: &str) -> Result<String, ProviderError> {
    match req.attributes.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(ProviderError::Rejected {
            provider: "aws".into(),
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
        assert_conforms(&AwsProvider::new());
    }

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
    fn rejects_missing_required_attr() {
        let p = AwsProvider::new();
        let r = req("aws_lambda_function", "fn");
        assert!(p.create(r).is_err());
    }

    #[test]
    fn computes_vpc_id_from_cidr() {
        let p = AwsProvider::new();
        let mut attributes = BTreeMap::new();
        attributes.insert("cidr_block".to_string(), Value::String("10.0.0.0/16".into()));
        let applied = p
            .create(ResourceRequest {
                id: "aws_vpc.demo".into(),
                type_name: "aws_vpc".into(),
                attributes,
            })
            .unwrap();
        assert_eq!(
            applied.attributes.get("id"),
            Some(&Value::String("vpc-1000016".into()))
        );
    }
}
