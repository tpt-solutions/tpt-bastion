# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT Bastion is an early-stage, from-scratch Rust rewrite of Terraform ("the Post-Terraform"),
built in response to HashiCorp's MPL→BSL relicensing. It is dual-licensed Apache-2.0/MIT and will
never be relicensed against the community. See `README.md` for the pitch and `todo.md` for the
full phased roadmap (Phase 0 through Phase 10, zero to public launch) — check `todo.md` before
starting work to see what phase is in progress and what's already checked off.

Only the DSL → IR compiler pipeline plus the execution-engine foundation exist today (Phases 0–3,
partially). The `bastion-engine` crate builds a dependency graph and a dry-run `plan` from IR. There
is no apply runtime, state management, provider system, or language bindings yet — don't assume they
exist.

## Commands

```sh
cargo build --workspace                 # build everything
cargo test --workspace                  # run all tests (unit tests live inline in each crate, e.g. bastion-dsl/src/lib.rs)
cargo test -p bastion-dsl               # test a single crate
cargo test -p bastion-dsl reports_lex_error_with_location   # run a single test by name
cargo fmt --all -- --check              # CI's fmt check
cargo clippy --workspace --all-targets -- -D warnings   # CI's lint check (warnings are errors)
cargo run --bin bastion -- compile examples/aws_s3.bcl  # compile a .bcl file to IR JSON
cargo run --bin bastion -- plan examples/aws_s3.bcl     # dry-run plan from a .bcl file
```

CI (`.github/workflows/ci.yml`) runs build+test and fmt+clippy (with `-D warnings`) as separate
jobs on every push/PR to `main`. Match this locally before considering work done.

## Architecture

Four crates under `crates/`, forming a strict pipeline: `bastion-dsl` depends on `bastion-ir`;
`bastion-engine` depends on `bastion-ir`; `bastion-cli` depends on `bastion-dsl` and
`bastion-engine`. Nothing depends on `bastion-cli`.

- **`bastion-ir`** — the versioned, serializable IR (`Document`, `Resource`, `Value`, etc.) that
  everything downstream (execution engine, state backends, providers, future TS/Python bindings)
  will consume instead of the source DSL. `IR_VERSION` is a `u32` bumped only on breaking shape
  changes; `is_compatible()` gates consumers on it. `Value` is the universal typed value
  (String/Number/Bool/Null/Reference/List/Map) — references (`my_bucket.bucket`) are kept as
  structured `Value::Reference { base, path }` rather than resolved, so a later execution engine
  can build the dependency graph from unevaluated IR.

- **`bastion-dsl`** — the `.bcl` configuration language, compiled through three stages that map
  directly to files: `lexer.rs` (source → `Token`s) → `parser.rs` (tokens → `ast::Ast`) →
  `compiler.rs` (AST → `bastion_ir::Document`). `lib.rs::compile_source()` is the single entry
  point that chains all three and is what the CLI and (eventually) language bindings call.
  `ast::Block` is a generic top-level block shape (`kind`, optional `type_name`/`name` labels,
  ordered attributes) shared by every block kind (`config`, `resource`, `module`, `variable`,
  `output`) — the compiler dispatches on `block.kind` and validates per-kind requirements (e.g.
  `resource` needs both a type and a name; duplicate resource ids/attributes/object keys are
  compile errors). All errors are `error::DslError`, split into `Lex`/`Parse`/`Compile` variants
  so the CLI can show the most relevant diagnostic; lex/parse errors carry line/col.

- **`bastion-engine`** — the execution core. `graph.rs::DepGraph` builds a dependency graph purely
  from the IR's structured `Value::Reference` values and topologically orders it (Kahn's
  algorithm) with cycle and unknown-reference detection; `plan.rs::Plan` walks that order into a
  sequence of `Action`s. `lib.rs::plan_document()` is the convenience entry point (validates IR
  compatibility, builds graph, returns a plan). There is no real apply yet — providers (Phase 6)
  are required to mutate infrastructure.

- **`bastion-cli`** — the `bastion` binary (clap-based). Has `compile` (`.bcl` → IR JSON) and
  `plan` (dependency-ordered dry-run plan) subcommands. Later roadmap phases (apply/state) will
  add subcommands here.

`examples/*.bcl` holds example DSL documents (`.bcl` extension) used as fixtures/demos — see
`aws_s3.bcl` for the current canonical shape of `config`/`variable`/`resource`/`output` blocks and
inter-resource references.

## Conventions

- Every source file starts with `// SPDX-License-Identifier: Apache-2.0 OR MIT`.
- Workspace-wide package metadata and shared dependency versions live in the root `Cargo.toml`
  under `[workspace.package]` / `[workspace.dependencies]`; per-crate `Cargo.toml` files should
  inherit (`.workspace = true`) rather than restate them.
- Each crate has its own `README.md` documenting its role and public API entry point.
