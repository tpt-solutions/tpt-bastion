# Bastion Provider Development Guide

This guide explains how to build a Bastion provider — the plugin that turns a
compiled `.bcl` document into real infrastructure. It covers the Phase 5 plugin
system in `crates/bastion-providers`.

## Vocabulary

- **SDK** — the `ProviderPlugin` trait you implement. It is the provider's
  contract with the engine.
- **ABI** — the wire protocol (`AbiRequest` / `AbiResponse`, line-delimited
  JSON) the host uses to talk to a plugin. You normally never touch it by hand;
  `PluginServer::serve` implements it for you.
- **Schema** — a `ProviderSchema` describing, in data, every resource type,
  its attributes, and which lifecycle ops are supported. Tooling reads it without
  running your provider.
- **Runtime** — the host abstraction that *loads* plugins. `NativeRuntime`
  runs them in-process (tests, built-ins); `SubprocessRuntime` runs each in its
  own OS process (isolation / sandboxing).

## The plugin trait

```rust
use bastion_providers::plugin::{DataSourceRequest, ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{AttributeSpec, LifecycleOps, ProviderSchema,
                                ResourceTypeSpec, ValueType};
use bastion_providers::error::ProviderError;
use bastion_engine::provider::AppliedResource;

pub struct MyProvider;

impl ProviderPlugin for MyProvider {
    fn name(&self) -> &str { "mycloud" }
    fn version(&self) -> &str { "0.1.0" }

    fn schema(&self) -> ProviderSchema {
        ProviderSchema::builder("mycloud", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("mycloud_bucket")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::computed("arn", ValueType::String))
                    .lifecycle(LifecycleOps { create: true, read: true,
                                update: true, delete: true })
                    .build(),
            )
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        // 1. Call the real mycloud API.
        // 2. Build the applied state (declared attrs + computed values).
        let mut attributes = req.attributes.clone();
        attributes.insert("arn".into(), bastion_ir::Value::String(format!("arn:{}", req.id)));
        Ok(AppliedResource { id: req.id, type_name: req.type_name, attributes })
    }

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {
        // Call the real mycloud API to destroy the resource.
        Ok(())
    }
}
```

`read`, `update`, and `data_source` have default impls that return
`UnsupportedOp`, so minimal providers only implement `name`, `version`,
`schema`, `create`, and `delete`.

## Serving the plugin (subprocess runtime)

The production runtime runs each provider as its own process and talks ABI over
stdin/stdout. Your crate's binary just serves the plugin:

```rust
fn main() {
    let provider = MyProvider;
    let _ = bastion_providers::abi::PluginServer::serve(
        &provider, std::io::stdin(), std::io::stdout());
}
```

## Loading the plugin (host side)

```rust
use bastion_providers::{NativeRuntime, PluginProvider, PluginRegistry};
use std::sync::Arc;

let runtime = NativeRuntime::new();
runtime.register(Arc::new(MyProvider));

// Wrap it as an engine `Provider` and apply through the executor:
let provider = PluginProvider::load(&runtime, "mycloud").unwrap();
let executor = Executor::new(provider, Box::new(PrintingHandler::default()));
let state = executor.apply(&doc, &plan, &State::new()).unwrap();
```

For isolation, point `SubprocessRuntime::new("path/to/my-provider-binary")` at
the compiled binary instead; everything else is identical.

## Scaffolding

```sh
cargo run --bin bastion -- provider scaffold mycloud
```

This emits `bastion-provider-mycloud/` with `lib.rs` (a working echo provider)
and `main.rs` (the server entry point). `cargo build` it, then load the binary
with `SubprocessRuntime`.

## Conventions

- Provider names are unique within a registry (`aws`, `gcp`, `local`, ...).
- Resource type names are provider-scoped (`aws_s3_bucket`).
- Computed attributes (ARNs, IDs) belong in the applied state returned from
  `create`, never in the document.
- Prefer failing fast in `schema()` when an unsupported type is requested; the
  engine surfaces `ProviderError` as an `EngineError::Provider`.
