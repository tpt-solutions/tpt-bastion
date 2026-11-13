# TPT Bastion — Project Roadmap

Next-generation Infrastructure-as-Code platform in Rust. Tracks all phases from zero to public release.

## Phase 0 — Project Foundations
- [x] Set up Rust workspace and crate layout (designed for independent crates.io publishing)
- [x] Set up CI pipeline (build, test, lint/clippy, fmt check)
- [x] Add dual license: LICENSE-APACHE + LICENSE-MIT, SPDX headers across crates
- [x] Define versioning/release strategy (semver)
- [x] Write initial README and project overview
- [x] Set up issue/task tracking conventions

## Phase 1 — DSL & IR Design
- [x] Design DSL syntax (type-safe, declarative infrastructure config)
- [x] Design type system (resource types, references, modules)
- [x] Design intermediate representation (IR) spec and versioning
- [x] Build parser (lexer, AST)
- [x] Build compiler: AST → IR
- [x] Implement compile-time error diagnostics
- [x] Write DSL language reference/spec doc

## Phase 2 — Language Bindings (TypeScript/Python)
- [x] Decide binding architecture (FFI, Wasm, or codegen from IR)
- [x] Implement TypeScript bindings
- [x] Implement Python bindings
- [ ] Publish binding packages (npm, PyPI)
- [x] Write example configs in DSL, TypeScript, and Python

## Phase 3 — Core Execution Engine
- [x] Build dependency graph construction from IR
- [x] Implement parallel, dependency-aware execution scheduler
- [x] Implement rollback support on failure
- [x] Implement plan/apply lifecycle
- [x] Implement dry-run and diff output
- [x] Implement execution logging/observability hooks

## Phase 4 — Native State Management
- [x] Design encrypted state file format
- [x] Implement state versioning/history
- [x] Implement state locking/concurrency control
- [ ] Implement real-time drift detection (plan-time only; no live refresh, see docs/status.md)
- [ ] Implement automatic drift reconciliation (reconcile plans exist; apply runs the mock provider)
- [x] Design pluggable interface for future external state backends (e.g. TPT Keystone)

## Phase 5 — Provider Plugin System
- [ ] Design Wasm-based plugin host/runtime (superseded by the subprocess ABI; no Wasm host)
- [x] Define provider SDK/ABI (resource CRUD, data sources, schema)
- [ ] Implement provider process isolation/sandboxing (Linux landlock path lacks a passing runtime test)
- [x] Define provider capability/schema format
- [x] Build provider scaffolding/starter template
- [x] Write provider development guide

## Phase 6 — Cloud & Platform Providers (parallel tracks)
- [ ] AWS provider (core resource coverage) — stub: no SDK calls (11.3)
- [ ] GCP provider (core resource coverage) — stub: no SDK calls (11.3)
- [ ] Azure provider (core resource coverage) — stub: no SDK calls (11.3)
- [ ] Kubernetes provider (core resource coverage) — stub: no API calls (11.3)
- [ ] Terraform-compatible provider shim (interop with existing TF providers) — experimental; real runs unverified

## Phase 7 — Event-Driven Updates
- [x] Design internal event source/trigger interface
- [x] Implement triggers for infrastructure updates on external events
- [x] Implement reconciliation-on-event loop
- [x] Design pluggable interface for future external event backends (e.g. TPT Flux)

## Phase 8 — Testing & Quality
- [x] Define unit/integration testing strategy across crates
- [x] Build provider conformance test suite
- [x] Build end-to-end plan/apply test scenarios
- [x] Add fuzzing/property tests for DSL compiler and IR
- [x] Set up test coverage reporting in CI

## Phase 9 — Packaging & Distribution
- [x] Prepare crates.io metadata for each publishable crate (description, keywords, categories, docs.rs config)
- [x] Set up `cargo publish` release automation
- [x] Build cross-platform CLI binaries
- [ ] Publish install script and package manager entries (Homebrew, cargo install, etc.) — Homebrew sha256 still placeholder (11.7)
- [x] Build and publish Docker image
- [x] Automate versioned GitHub releases with changelogs

## Phase 10 — Open Source Launch
- [x] Finalize dual Apache-2.0/MIT licensing and notices across all crates
- [x] Build public documentation site
- [x] Write getting-started guide
- [x] Write migration guide from Terraform/OpenTofu
- [ ] Publish public repository
- [ ] Public launch announcement

## Phase 11 — Hardening & Honesty (post-review)
Findings from the project review. Several earlier `[x]` items above are overstated (real apply, cloud providers, Wasm host, live drift, install/brew entries); these tasks close the gaps.

### 11.1 Correctness & security fixes
- [x] Events webhook + UI: compare secret/token with original case (only lowercase header names); fix webhook POST response; remove dead `header_len`
- [x] UI: add Origin/Host (CSRF) check on POST `/api/apply`; show file label from `/api/info`
- [x] Sandbox: make landlock policy actually read-only outside allowed paths; stop swallowing errors; document Windows/macOS limits
- [x] terraform-shim: per-resource workdir, persist tfstate, make `delete` really destroy, serialize per dir
- [x] Commit `todo 1260715.md` → `history/` move; stop ignoring `Cargo.lock` (the lock file itself is committed with the next commit)

### 11.2 Engine & provider plumbing
- [x] Extend `Provider` trait with `read` and `update`; add `ProviderRouter` (type prefix → provider)
- [x] Attribute-level plan diff (Create/Update/Replace/NoOp/Delete, "known after apply")
- [x] Executor: real update path, no-op for unchanged, rollback on delete failure, preserve untouched resources in state
- [ ] `bastion apply` uses real providers (MockProvider only behind `--mock`); add `destroy` and `refresh`
- [ ] Live drift detection via provider `read` (engine `refresh` exists; `drift` does not use it yet)
- [ ] Provider config (region/credentials) from `config` block
- [x] `bastion events` accepts `--state` and passphrase options

### 11.3 Real cloud providers (replace `TODO(sdk)` echo stubs)
- [ ] Kubernetes: real create/read/update/delete for all 6 resource types
- [ ] AWS: real SDK calls for all 7 resource types; real ARNs; fix `aws_dynamodb_table` missing id; remove hardcoded account/region
- [ ] GCP: real calls for all 6 resource types
- [ ] Azure: real calls for all 6 resource types
- [ ] Gated integration tests (kind, LocalStack, emulators) in a separate CI job

### 11.4 Core CLI adoption tooling
- [x] `bastion init` project scaffolding
- [x] `bastion validate`
- [x] `bastion fmt` (+ `--check`)
- [x] `bastion doctor`
- [x] `bastion completions` (bash/zsh/fish/powershell)
- [x] `bastion graph` (DOT/mermaid) and `bastion output`
- [x] Colored plan diff, `--json` plan output, saved plan files (`--out`)
- [x] Source-span diagnostics with caret snippets and "did you mean" (incl. compile errors)
- [x] Compile-time reference validation
- [ ] DSL: `count`/`for_each` expansion, variable default/usage checks, module expansion
- [x] `plan --explain` (why each change happens)
- [x] Policy-as-code: `policy {}` rules evaluated at plan time

### 11.5 Terraform migration
- [x] New `tpt-bastion-import` crate: HCL2 subset `.tf` → `.bcl` converter with unsupported-construct reporting (subset only; see the migration guide)
- [x] `bastion import tf <dir>`
- [x] `bastion import state <terraform.tfstate>` → Bastion encrypted state (with `--passphrase-env`)
- [x] `bastion state rm/mv/pull/push`
- [x] Golden-file migration examples in `examples/migration/`; update migration guide

### 11.6 Editor support
- [x] VS Code extension: TextMate grammar, language config, snippets, `.vsix` build in CI
- [x] `bastion-lsp` (diagnostics, formatting, hover/go-to-definition, resource-type completion) + `bastion lsp`
- [x] `bastion provider schema <name> --json` and generated provider docs

### 11.7 Release & install pipeline
- [x] Consistent release asset names; fix `install.sh` / `install.ps1` 404s
- [x] Publish `SHA256SUMS` + signatures; verify checksums in install scripts; musl detection
- [x] Fix `cargo publish` order (add `bastion-events`, `bastion-ui`), remove `|| true` / `--no-verify`
- [ ] Homebrew formula: real sha256 (auto-bump in release workflow); winget/scoop manifests
- [ ] `setup-bastion` GitHub Action; Docker image smoke test in CI
- [ ] CI: Windows/macOS test matrix, MSRV, cargo-deny/audit
- [x] TypeScript binding: build to `dist`, derive `ir_version` from the Rust source (also Python)
- [ ] Publish TypeScript and Python bindings to npm and PyPI (needs registry tokens)

### 11.8 Docs honesty
- [x] Rewrite `CLAUDE.md`, `README.md` crate table/layout, crate and provider READMEs to match reality
- [x] Replace "Wasm" claims with "sandboxed subprocess provider ABI" (or build the Wasm host)
- [x] Remove "real-time drift" wording until live refresh ships; add provider/resource status matrix
- [x] Correct over-checked items in Phases 3–6 and 9
