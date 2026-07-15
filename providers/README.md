# SPDX-License-Identifier: Apache-2.0 OR MIT

# Bastion provider tracks (Phase 6)

Independent, publishable provider crates that implement the
[`ProviderPlugin`](https://docs.rs/bastion-providers) SDK. Each is loaded by the
Bastion engine the same way as the built-in `null` provider.

| Crate                        | Provider     | Status                                                        |
| ---------------------------- | ------------ | ------------------------------------------------------------- |
| `providers/aws`              | `aws`        | Initial: `aws_s3_bucket`, `aws_dynamodb_table` schemas + lifecycle. Real `aws-sdk-*` calls stubbed (`TODO(sdk)`). |
| `providers/gcp`              | `google`     | Initial: `google_storage_bucket` schema + lifecycle. Real SDK calls stubbed. |
| `providers/azure`            | `azurerm`    | Initial: `azurerm_resource_group` schema + lifecycle. Real SDK calls stubbed. |
| `providers/kubernetes`       | `kubernetes` | Initial: `kubernetes_namespace` schema + lifecycle. Real `kube` calls stubbed. |
| `providers/terraform-shim`   | `terraform-shim` | Working interop: renders HCL and drives `terraform`/`tofu`. |

## What "initial coverage" means

Each cloud provider ships a **correct schema, lifecycle contract, and engine
integration** — the `create`/`delete` path records applied state and computes
output attributes. The actual cloud API calls are marked `// TODO(sdk)`: they
are echo/record stubs so the toolchain, conformance tests, and the engine all
work end-to-end without cloud credentials. Swapping in the real SDK is
mechanical and per-resource.

## Wiring a provider into the CLI

Built-in providers register through a `NativeRuntime` (see
`bastion provider list`). For isolated, production use, ship a provider as a
subprocess plugin via the provider ABI (`SubprocessRuntime`) and load it from
the registry. The `terraform-shim` additionally requires `terraform` or `tofu`
on `PATH` (override with `BASTION_TF_BIN`).

## Roadmap

- Expand each provider's resource catalog toward parity with the common Terraform
  resources (compute, networking, IAM, databases).
- Replace `TODO(sdk)` echo paths with real, credential-aware SDK calls.
- Add a gRPC-based Terraform plugin protocol client so the shim can consume
  upstream provider schemas natively (no `terraform` binary required).
