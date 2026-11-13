# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT Bastion is an early-stage, from-scratch Rust rewrite of Terraform ("the Post-Terraform"),
built in response to HashiCorp's MPL→BSL relicensing. It is dual-licensed Apache-2.0/MIT and will
never be relicensed against the community. See `README.md` for the pitch, `docs/status.md` for
what works today feature by feature, and `todo.md` for the phased roadmap. Check `todo.md`
(Phase 11 is the current hardening pass) before starting work.

What exists: the DSL → IR compiler, a dependency-ordered plan with attribute-level diffs, the
execution engine (create, update, destroy, rollback), encrypted native state with history and
locking, plan-time drift detection, a subprocess provider ABI with sandboxing, event sources,
a localhost dashboard, and TypeScript/Python bindings.

What does not exist, or is only a stub. Do not assume these work:
- `bastion apply` runs `MockProvider`. It never changes real infrastructure.
- The AWS, GCP, Azure, and Kubernetes providers are stubs: no cloud API calls.
- Drift detection does not query infrastructure. Provider `read` is not wired into it.
- There is no Wasm host. Provider isolation is a subprocess.

## Commands

```sh
cargo build --workspace                 # build everything
cargo test --workspace                  # run all tests (unit tests live inline in each crate, e.g. tpt-bastion-dsl/src/lib.rs)
cargo test -p tpt-bastion-dsl               # test a single crate
cargo test -p tpt-bastion-dsl reports_lex_error_with_location   # run a single test by name
cargo fmt --all -- --check              # CI's fmt check
cargo clippy --workspace --all-targets -- -D warnings   # CI's lint check (warnings are errors)
cargo run --bin bastion -- compile examples/aws_s3.bcl  # compile a .bcl file to IR JSON
cargo run --bin bastion -- plan examples/aws_s3.bcl     # dry-run plan from a .bcl file
```

CI (`.github/workflows/ci.yml`) runs build+test and fmt+clippy (with `-D warnings`) as separate
jobs on every push/PR to `master`. Match this locally before considering work done.

## Architecture

Core crates live under `crates/`: `ir`, `dsl`, `engine`, `state`, `providers`, `events`, `ui`, and
`cli`. `providers/` holds the cloud and Terraform-shim provider crates, which build on the
`providers` crate. `bindings/` holds the TypeScript and Python packages. Nothing depends on
`tpt-bastion-cli`. Dependencies flow `ir` → `dsl`/`engine` → `state`/`providers` → `ui`/`cli`.

- **`tpt-bastion-ir`** — the versioned, serializable IR (`Document`, `Resource`, `Value`, etc.) that
  everything downstream (execution engine, state backends, providers, future TS/Python bindings)
  will consume instead of the source DSL. `IR_VERSION` is a `u32` bumped only on breaking shape
  changes; `is_compatible()` gates consumers on it. `Value` is the universal typed value
  (String/Number/Bool/Null/Reference/List/Map) — references (`my_bucket.bucket`) are kept as
  structured `Value::Reference { base, path }` rather than resolved, so a later execution engine
  can build the dependency graph from unevaluated IR.

- **`tpt-bastion-dsl`** — the `.bcl` configuration language, compiled through three stages that map
  directly to files: `lexer.rs` (source → `Token`s) → `parser.rs` (tokens → `ast::Ast`) →
  `compiler.rs` (AST → `tpt_bastion_ir::Document`). `lib.rs::compile_source()` is the single entry
  point that chains all three and is what the CLI and (eventually) language bindings call.
  `ast::Block` is a generic top-level block shape (`kind`, optional `type_name`/`name` labels,
  ordered attributes) shared by every block kind (`config`, `resource`, `module`, `variable`,
  `output`) — the compiler dispatches on `block.kind` and validates per-kind requirements (e.g.
  `resource` needs both a type and a name; duplicate resource ids/attributes/object keys are
  compile errors). All errors are `error::DslError`, split into `Lex`/`Parse`/`Compile` variants
  so the CLI can show the most relevant diagnostic; lex/parse errors carry line/col.

- **`tpt-bastion-engine`** — the execution core. `graph.rs::DepGraph` builds a dependency graph purely
  from the IR's structured `Value::Reference` values and topologically orders it (Kahn's
  algorithm) with cycle and unknown-reference detection. `plan.rs::Plan` diffs declared attributes
  against prior state into `create`/`update`/`delete`/`no-op` actions. `provider.rs` defines the
  `Provider` trait (`create`, `update`, `read`, `destroy`) and `ProviderRouter`, which routes by type
  prefix. `executor.rs::Executor` applies a plan level by level and undoes earlier changes on failure.

- **`tpt-bastion-state`** — native state: encrypted or plaintext files, history, locking, drift
  (`drift.rs`, plan-time only), and reconcile plans (`reconcile.rs`).

- **`tpt-bastion-providers`** — the provider ABI (line-delimited JSON, `abi.rs`), the subprocess
  runtime with resource limits and Linux landlock (`sandbox.rs`), the plugin registry, schemas, and
  scaffolding. The cloud providers in `providers/` implement its `ProviderPlugin` trait.

- **`tpt-bastion-events`** and **`tpt-bastion-ui`** — event sources (file changes, authenticated
  webhooks) and the localhost dashboard. The dashboard refuses cross-origin POSTs.

- **`tpt-bastion-cli`** — the `bastion` binary (clap-based). Subcommands: `compile`, `plan`
  (`--json`), `apply`, `state`, `drift`, `provider`, `events`, `ui`, `validate`, `graph`, `output`,
  `doctor`, `init`, `completions`.

`examples/*.bcl` holds example DSL documents (`.bcl` extension) used as fixtures/demos — see
`aws_s3.bcl` for the current canonical shape of `config`/`variable`/`resource`/`output` blocks and
inter-resource references.

## Conventions

- Every source file starts with `// SPDX-License-Identifier: Apache-2.0 OR MIT`.
- Workspace-wide package metadata and shared dependency versions live in the root `Cargo.toml`
  under `[workspace.package]` / `[workspace.dependencies]`; per-crate `Cargo.toml` files should
  inherit (`.workspace = true`) rather than restate them.
- Each crate has its own `README.md` documenting its role and public API entry point.
- Every crate's package name starts with `tpt-bastion-` (e.g. `tpt-bastion-dsl`, `tpt-bastion-provider-aws`);
  the Rust import path uses underscores (`tpt_bastion_dsl`). The `bastion` binary name is unchanged.
- Each crate has its own `README.md`, `CHANGELOG.md` (Keep a Changelog format), explicit `keywords`
  (max 5) and `categories` (valid crates.io slugs) in its `Cargo.toml`.
