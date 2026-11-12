# tpt-bastion-engine

The execution core for [TPT Bastion](https://bastion.tpt.dev) — the
community-owned, "Post-Terraform" Infrastructure-as-Code platform.

`tpt-bastion-engine` consumes a compiled [`tpt_bastion_ir`] document and turns it into a
dependency graph and a concrete plan:

- `graph::DepGraph` builds a dependency graph from the structured references the
  compiler preserves in the IR, then topologically orders nodes so a resource is
  acted on only after everything it references already exists. Cycle and
  unknown-reference detection are built in.
- `plan::Plan` walks that order and emits the sequence of `Action`s the apply
  stage (and future providers) will execute.
- `plan_document` is the single convenience entry point used by the CLI `plan`
  subcommand.

This crate is part of the strict `tpt-bastion-ir` → `tpt-bastion-dsl` → `tpt-bastion-cli`
pipeline and depends only on `tpt-bastion-ir`. It does not depend on the DSL or CLI.

## Example

```rust
use tpt_bastion_dsl::compile_source;
use tpt_bastion_engine::plan_document;

let doc = compile_source(source).unwrap();
let plan = plan_document(&doc).unwrap();
print!("{plan}", plan = plan.render());
```

[`tpt_bastion_ir`]: https://crates.io/crates/tpt-bastion-ir
