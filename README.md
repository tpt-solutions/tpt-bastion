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

This repository is at an early stage. The foundation (workspace, licensing,
CI) and the DSL → IR compiler pipeline are in place:

| Crate           | Purpose                                            |
| --------------- | -------------------------------------------------- |
| `bastion-ir`    | The versioned intermediate representation.         |
| `bastion-dsl`   | The DSL lexer, parser, and AST→IR compiler.        |
| `bastion-cli`   | The `bastion` command-line tool.                   |

## Quick start

```sh
cargo run --bin bastion -- compile examples/aws_s3.bcl
```

This reads `examples/aws_s3.bcl`, compiles it through the DSL pipeline, and
prints the resulting IR as JSON.

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
