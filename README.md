# TPT Bastion

> The Post-Terraform. Infrastructure as Code that you actually own.

TPT Bastion is a next-generation Infrastructure-as-Code (IaC) platform built
from scratch in Rust for the cloud-native era. It was created in response to
HashiCorp's 2023 switch of Terraform from MPL to BSL — a move that broke the
open-source promise thousands of companies relied on. Bastion is released
under **Apache-2.0 OR MIT** so the license can *never* be restricted against
the community.

## Why Bastion?

- **Type-safe DSL** — a modern, declarative configuration language that
  compiles to a stable intermediate representation (IR), catching errors at
  *compile time* instead of at apply time.
- **Open by default** — permissively dual-licensed; no BSL, no surprise
  relicensing.
- **Native state** — encrypted, versioned state files with no separate backend
  required (pluggable for [TPT Keystone] later).
- **Plan-time drift detection** against recorded state. Live refresh from
  infrastructure is not built yet.
- **Subprocess provider isolation** — each provider runs as its own process,
  with resource limits and (on Linux) a read-only filesystem policy. A Wasm
  host is not built.
- **Event-driven updates** — plan or apply when files change or a webhook
  fires.
- **Language bindings** — compile the same DSL from TypeScript or Python.

## Status

Bastion is under active development. The DSL, plan, native state, and event
pipeline work. `bastion apply` runs an in-memory mock provider, so it does not
change real infrastructure. The AWS, GCP, Azure, and Kubernetes providers are
stubs that do not call their cloud APIs.

The full, feature-by-feature account is in [`docs/status.md`](docs/status.md).

| Crate                   | Purpose                                                       |
| ----------------------- | ------------------------------------------------------------- |
| `tpt-bastion-ir`        | The versioned intermediate representation.                    |
| `tpt-bastion-dsl`       | The DSL lexer, parser, and AST→IR compiler.                   |
| `tpt-bastion-engine`    | Dependency graph, planning, and parallel execution core.      |
| `tpt-bastion-state`     | Encrypted, versioned, lock-aware native state and drift.      |
| `tpt-bastion-providers` | Provider ABI, subprocess runtime, sandboxing, and registry.   |
| `tpt-bastion-events`    | File-change and webhook event sources.                        |
| `tpt-bastion-ui`        | The localhost web dashboard.                                  |
| `tpt-bastion-cli`       | The `bastion` command-line tool.                              |

See [`docs/`](docs) for the language reference, provider guide, binding
architecture, getting started, and the Terraform migration guide.

## Quick start

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
```

This reads `examples/aws_s3.bcl`, compiles it through the DSL pipeline, and
prints the resulting IR as JSON.

## Installation

Pick whichever fits your workflow:

```sh
# Install script (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/tpt/bastion/master/scripts/install.sh | sh

# Install script (Windows, PowerShell)
irm https://raw.githubusercontent.com/tpt/bastion/master/scripts/install.ps1 | iex

# From source
cargo install --path crates/tpt-bastion-cli

# Docker
docker run --rm ghcr.io/tpt/bastion:latest --help
```

Homebrew, npm, and PyPI packages are published at release time; see
[`docs/bindings.md`](docs/bindings.md) for the language bindings.

## Project layout

```
crates/
  tpt-bastion-ir/         IR types, versioning, serde
  tpt-bastion-dsl/        DSL lexer, parser, AST, compiler
  tpt-bastion-engine/     Dependency graph, plan, executor
  tpt-bastion-state/      Native state, drift, reconcile
  tpt-bastion-providers/  Provider ABI, subprocess runtime, sandbox
  tpt-bastion-events/     Event sources (files, webhooks)
  tpt-bastion-ui/         Localhost dashboard
  tpt-bastion-cli/        `bastion` CLI entry point
providers/                Cloud and Terraform-shim providers (stubs / experimental)
bindings/                 TypeScript and Python bindings
examples/                 Example .bcl documents
```

## Roadmap

See [`todo.md`](todo.md) for the full phased roadmap from zero to public
launch (DSL, language bindings, execution engine, state management, provider
system, cloud providers, event-driven updates, testing, packaging, and open
source launch).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

[TPT Keystone]: https://bastion.tpt.dev/keystone
[TPT Flux]: https://bastion.tpt.dev/flux
