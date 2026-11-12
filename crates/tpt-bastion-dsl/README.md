# tpt-bastion-dsl

The configuration language for [TPT Bastion][bastion]: a type-safe, declarative
Infrastructure-as-Code DSL that compiles to the stable
[`tpt_bastion_ir`](../tpt-bastion-ir) intermediate representation.

The pipeline is three stages — lex, parse, compile — and most callers only
need the `compile_source` entry point:

```rust
use tpt_bastion_dsl::compile_source;

let src = r#"
    config { name = "demo" region = "us-east-1" }
    resource aws_s3_bucket demo { bucket = "demo-bucket" }
"#;
let doc = compile_source(src).unwrap();
assert_eq!(doc.resources.len(), 1);
```

See the workspace [README](../../README.md) for the project overview.

## License

Apache-2.0 OR MIT, at your option.

[bastion]: https://bastion.tpt.dev
