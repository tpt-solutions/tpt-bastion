# tpt-bastion-provider-aws

AWS provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `aws_s3_bucket` and `aws_dynamodb_table`
schemas and a `create`/`delete` lifecycle that records state. Nothing calls AWS yet:
the outputs (`id`, `arn`) are generated locally, and real `aws-sdk-*` calls are stubbed behind
`// TODO(sdk)` markers.

```rust
use tpt_bastion_providers::plugin::NativeRuntime;
use tpt_bastion_provider_aws::AwsProvider;
use std::sync::Arc;

let runtime = NativeRuntime::new();
runtime.register(Arc::new(AwsProvider::new()));
```

See [`../README.md`](../README.md) for the status of all provider tracks.
