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
- **Real-time drift detection** with automatic reconciliation.
- **Wasm provider sandboxing** — providers run isolated, with first-class
  support for AWS, GCP, Azure, Kubernetes, and Terraform-compatible providers.
- **Event-driven updates** — reconcile infrastructure on external events
  (via [TPT Flux]).
- **Language bindings** — drive your infrastructure from TypeScript or Python
  against the same IR.

## Status

Bastion is under active development. The core platform is in place and the CLI
is runnable end-to-end: compile → plan → apply, with encrypted native state,
drift detection, a Wasm-ready provider runtime, and language bindings.

| Crate             | Purpose                                                       |
| ----------------- | ------------------------------------------------------------- |
| `bastion-ir`      | The versioned intermediate representation.                   |
| `bastion-dsl`     | The DSL lexer, parser, and AST→IR compiler.                  |
| `bastion-engine`  | Dependency graph, planning, and parallel execution core.     |
| `bastion-state`   | Encrypted, versioned, lock-aware native state management.    |
| `bastion-providers` | Provider SDK, schema, runtime, registry, and scaffolding.  |
| `bastion-cli`     | The `bastion` command-line tool.                             |

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
curl -fsSL https://raw.githubusercontent.com/tpt/bastion/main/scripts/install.sh | sh

# Install script (Windows, PowerShell)
irm https://raw.githubusercontent.com/tpt/bastion/main/scripts/install.ps1 | iex

# From source
cargo install --path crates/bastion-cli

# Docker
docker run --rm ghcr.io/tpt/bastion:latest --help
```

Homebrew, npm, and PyPI packages are published at release time; see
[`docs/bindings.md`](docs/bindings.md) for the language bindings.

## Project layout

```
crates/
  bastion-ir/     IR types, versioning, serde
  bastion-dsl/    DSL lexer, parser, AST, compiler
  bastion-cli/    CLI entry point
examples/         Example .bcl documents
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
