# TPT Bastion

> The Post-Terraform. Infrastructure as Code that you actually own.

Bastion is a next-generation Infrastructure-as-Code (IaC) platform built from
scratch in Rust. It was created in response to HashiCorp's 2023 switch of
Terraform from MPL to BSL — a move that broke the open-source promise thousands
of companies relied on. Bastion is released under **Apache-2.0 OR MIT** so the
license can *never* be restricted against the community.

## Why Bastion?

- **Type-safe DSL** — a modern, declarative configuration language that
  compiles to a stable intermediate representation (IR), catching errors at
  *compile time* instead of at apply time.
- **Open by default** — permissively dual-licensed; no BSL, no surprise
  relicensing.
- **Native state** — encrypted, versioned state files with no separate backend
  required (pluggable for [TPT Keystone] later).
- **Real-time drift detection** with automatic reconciliation.
- **Wasm provider sandboxing** — providers run isolated, with first-class
  support for AWS, GCP, Azure, Kubernetes, and Terraform-compatible providers.
- **Event-driven updates** — reconcile infrastructure on external events.

## Start here

- [Getting Started](getting-started.md) — from zero to an applied stack.
- [Language Reference](language-reference.md) — the full `.bcl` syntax.
- [Provider Guide](provider-guide.md) — build and ship a provider.
- [Binding Architecture](bindings.md) — drive Bastion from TypeScript/Python.
- [Events](events.md) — reconcile on file changes, webhooks, and more.
- [Migrating from Terraform / OpenTofu](migrate-from-terraform.md).

## Quick start

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
```

This reads `examples/aws_s3.bcl`, compiles it through the DSL pipeline, and
prints the resulting IR as JSON.

[TPT Keystone]: https://bastion.tpt.dev/keystone
