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
- [ ] Decide binding architecture (FFI, Wasm, or codegen from IR)
- [ ] Implement TypeScript bindings
- [ ] Implement Python bindings
- [ ] Publish binding packages (npm, PyPI)
- [ ] Write example configs in DSL, TypeScript, and Python

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
- [x] Implement real-time drift detection
- [x] Implement automatic drift reconciliation
- [x] Design pluggable interface for future external state backends (e.g. TPT Keystone)

## Phase 5 — Provider Plugin System
- [x] Design Wasm-based plugin host/runtime
- [x] Define provider SDK/ABI (resource CRUD, data sources, schema)
- [x] Implement provider process isolation/sandboxing
- [x] Define provider capability/schema format
- [x] Build provider scaffolding/starter template
- [x] Write provider development guide

## Phase 6 — Cloud & Platform Providers (parallel tracks)
- [ ] AWS provider (core resource coverage)
- [ ] GCP provider (core resource coverage)
- [ ] Azure provider (core resource coverage)
- [ ] Kubernetes provider (core resource coverage)
- [ ] Terraform-compatible provider shim (interop with existing TF providers)

## Phase 7 — Event-Driven Updates
- [ ] Design internal event source/trigger interface
- [ ] Implement triggers for infrastructure updates on external events
- [ ] Implement reconciliation-on-event loop
- [ ] Design pluggable interface for future external event backends (e.g. TPT Flux)

## Phase 8 — Testing & Quality
- [x] Define unit/integration testing strategy across crates
- [x] Build provider conformance test suite
- [x] Build end-to-end plan/apply test scenarios
- [x] Add fuzzing/property tests for DSL compiler and IR
- [x] Set up test coverage reporting in CI

## Phase 9 — Packaging & Distribution
- [ ] Prepare crates.io metadata for each publishable crate (description, keywords, categories, docs.rs config)
- [ ] Set up `cargo publish` release automation
- [ ] Build cross-platform CLI binaries
- [ ] Publish install script and package manager entries (Homebrew, cargo install, etc.)
- [ ] Build and publish Docker image
- [ ] Automate versioned GitHub releases with changelogs

## Phase 10 — Open Source Launch
- [ ] Finalize dual Apache-2.0/MIT licensing and notices across all crates
- [ ] Build public documentation site
- [ ] Write getting-started guide
- [ ] Write migration guide from Terraform/OpenTofu
- [ ] Publish public repository
- [ ] Public launch announcement
