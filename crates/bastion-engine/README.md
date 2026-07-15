# bastion-engine

The execution core for [TPT Bastion](https://bastion.tpt.dev) — the
community-owned, "Post-Terraform" Infrastructure-as-Code platform.

`bastion-engine` consumes a compiled [`bastion_ir`] document and turns it into a
dependency graph and a concrete plan:

- `graph::DepGraph` builds a dependency graph from the structured references the
  compiler preserves in the IR, then topologically orders nodes so a resource is
  acted on only after everything it references already exists. Cycle and
  unknown-reference detection are built in.
- `plan::Plan` walks that order and emits the sequence of `Action`s the apply
  stage (and future providers) will execute.
- `plan_document` is the single convenience entry point used by the CLI `plan`
  subcommand.

This crate is part of the strict `bastion-ir` → `bastion-dsl` → `bastion-cli`
pipeline and depends only on `bastion-ir`. It does not depend on the DSL or CLI.

## Example

```rust
use bastion_dsl::compile_source;
use bastion_engine::plan_document;

let doc = compile_source(source).unwrap();
let plan = plan_document(&doc).unwrap();
print!("{plan}", plan = plan.render());
```

[`bastion_ir`]: https://crates.io/crates/bastion-ir
