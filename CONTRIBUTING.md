# Contributing to TPT Bastion

Thank you for your interest in Bastion — the community-owned, "Post-Terraform"
Infrastructure-as-Code platform.

**Contributions are accepted through issues only.** Please do not open pull
requests. Instead, describe the bug, feature, or change you would like in an
issue, and the maintainers will take it from there.

## Code of Conduct

Be respectful and constructive. We want Bastion to be a welcoming project for
everyone who depends on open infrastructure tooling.

## Licensing

Bastion is dual-licensed under **Apache-2.0 OR MIT**. Every Rust source file
must begin with:

```rust
// SPDX-License-Identifier: Apache-2.0 OR MIT
```

## Project layout

Bastion is a Cargo workspace of independently-publishable crates under `crates/`
(with providers under `providers/`), forming a strict pipeline:

- `tpt-bastion-ir` — versioned, serializable intermediate representation.
- `tpt-bastion-dsl` — the `.bcl` language: lexer → parser → compiler.
- `tpt-bastion-cli` — the `bastion` binary.
- `tpt-bastion-engine` — dependency graph, planning, and execution core.

`tpt-bastion-dsl` depends on `tpt-bastion-ir`; `tpt-bastion-cli` depends on
`tpt-bastion-dsl` and `tpt-bastion-engine`. Nothing depends on `tpt-bastion-cli`.

## Development workflow

For reproducing issues locally:

```sh
cargo build --workspace                                      # build everything
cargo test --workspace                                       # run all tests
cargo test -p tpt-bastion-dsl                                # test one crate
cargo fmt --all                                              # format
cargo clippy --workspace --all-targets -- -D warnings        # lint (warnings are errors)
cargo run --bin bastion -- compile examples/aws_s3.bcl       # compile a .bcl document
```

CI runs build+test and fmt+clippy (with `-D warnings`) on every push and pull
request to `master`.

## Reporting issues

Use the issue templates (`bug_report`, `feature_request`) and check `todo.md`
first to see the current phase and whether the work is already planned. Keep
each issue to a single concern.

For security vulnerabilities, use GitHub's private security advisory form
rather than a public issue.
