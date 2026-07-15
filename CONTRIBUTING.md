# Contributing to TPT Bastion

Thank you for your interest in contributing to Bastion — the community-owned,
"Post-Terraform" Infrastructure-as-Code platform. This document explains how we
work so your contribution can land smoothly.

## Code of Conduct

Be respectful and constructive. We want Bastion to be a welcoming project for
everyone who depends on open infrastructure tooling.

## Licensing

Bastion is dual-licensed under **Apache-2.0 OR MIT**. By contributing, you agree
that your contributions are licensed under the same terms. Every Rust source
file must begin with:

```rust
// SPDX-License-Identifier: Apache-2.0 OR MIT
```

## Project layout

Bastion is a Cargo workspace of independently-publishable crates under `crates/`
forming a strict pipeline:

- `bastion-ir` — versioned, serializable intermediate representation.
- `bastion-dsl` — the `.bcl` language: lexer → parser → compiler.
- `bastion-cli` — the `bastion` binary.
- `bastion-engine` — dependency graph, planning, and execution core.

`bastion-dsl` depends on `bastion-ir`; `bastion-cli` depends on `bastion-dsl`
and `bastion-engine`. Nothing depends on `bastion-cli`.

## Before you start

1. Check `todo.md` to see the current phase and whether the work is already
   planned. Open an issue to discuss anything beyond a small bug fix.
2. Keep changes scoped to a single concern; large refactors are easier to review
   as a series of smaller PRs.

## Development workflow

```sh
cargo build --workspace                                      # build everything
cargo test --workspace                                       # run all tests
cargo test -p bastion-dsl                                    # test one crate
cargo fmt --all                                              # format
cargo clippy --workspace --all-targets -- -D warnings        # lint (warnings are errors)
cargo run --bin bastion -- compile examples/aws_s3.bcl      # compile a .bcl document
```

CI runs build+test and fmt+clippy (with `-D warnings`) on every push/PR to
`main`. Your PR must pass both jobs. Match them locally before requesting review.

## Commit and PR guidelines

- Write clear, imperative commit messages (`Add dependency graph builder`).
- Open PRs against `main` using the PR template.
- Reference the `todo.md` phase your work advances.
- Keep PRs focused; describe the *why*, not just the *what*.

## Reporting issues

Use the issue templates (`bug_report`, `feature_request`). For security issues,
file a private security advisory rather than a public issue.
