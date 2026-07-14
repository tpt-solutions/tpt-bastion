# TPT Bastion — Project Roadmap

Next-generation Infrastructure-as-Code platform in Rust. Tracks all phases from zero to public release.

## Phase 0 — Project Foundations
- [ ] Set up Rust workspace and crate layout (designed for independent crates.io publishing)
- [ ] Set up CI pipeline (build, test, lint/clippy, fmt check)
- [ ] Add dual license: LICENSE-APACHE + LICENSE-MIT, SPDX headers across crates
- [ ] Define versioning/release strategy (semver)
- [ ] Write initial README and project overview
- [ ] Set up issue/task tracking conventions

## Phase 1 — DSL & IR Design
- [ ] Design DSL syntax (type-safe, declarative infrastructure config)
- [ ] Design type system (resource types, references, modules)
- [ ] Design intermediate representation (IR) spec and versioning
- [ ] Build parser (lexer, AST)
- [ ] Build compiler: AST → IR
- [ ] Implement compile-time error diagnostics
- [ ] Write DSL language reference/spec doc

## Phase 2 — Language Bindings (TypeScript/Python)
- [ ] Decide binding architecture (FFI, Wasm, or codegen from IR)
- [ ] Implement TypeScript bindings
- [ ] Implement Python bindings
- [ ] Publish binding packages (npm, PyPI)
- [ ] Write example configs in DSL, TypeScript, and Python

## Phase 3 — Core Execution Engine
- [ ] Build dependency graph construction from IR
- [ ] Implement parallel, dependency-aware execution scheduler
- [ ] Implement rollback support on failure
- [ ] Implement plan/apply lifecycle
- [ ] Implement dry-run and diff output
- [ ] Implement execution logging/observability hooks

## Phase 4 — Native State Management
- [ ] Design encrypted state file format
- [ ] Implement state versioning/history
- [ ] Implement state locking/concurrency control
- [ ] Implement real-time drift detection
- [ ] Implement automatic drift reconciliation
- [ ] Design pluggable interface for future external state backends (e.g. TPT Keystone)

## Phase 5 — Provider Plugin System
- [ ] Design Wasm-based plugin host/runtime
- [ ] Define provider SDK/ABI (resource CRUD, data sources, schema)
- [ ] Implement provider process isolation/sandboxing
- [ ] Define provider capability/schema format
- [ ] Build provider scaffolding/starter template
- [ ] Write provider development guide

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
- [ ] Define unit/integration testing strategy across crates
- [ ] Build provider conformance test suite
- [ ] Build end-to-end plan/apply test scenarios
- [ ] Add fuzzing/property tests for DSL compiler and IR
- [ ] Set up test coverage reporting in CI

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
