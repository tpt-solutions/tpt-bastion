# bastion-provider-aws

AWS provider for TPT Bastion (Phase 6 — core resource coverage).

Implements the `ProviderPlugin` SDK with `aws_s3_bucket` and `aws_dynamodb_table`
schemas and a working `create`/`delete` lifecycle that records applied state and
computes outputs (`id`, `arn`). Real `aws-sdk-*` calls are stubbed behind
`// TODO(sdk)` markers.

```rust
use bastion_providers::plugin::NativeRuntime;
use bastion_provider_aws::AwsProvider;
use std::sync::Arc;

let runtime = NativeRuntime::new();
runtime.register(Arc::new(AwsProvider::new()));
```

See [`../README.md`](../README.md) for the status of all provider tracks.
