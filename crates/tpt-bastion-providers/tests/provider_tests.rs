// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for the Bastion provider plugin system (Phase 5).

use std::collections::BTreeMap;
use std::sync::Arc;
use tpt_bastion_engine::provider::AppliedResource;
use tpt_bastion_engine::{Executor, NoopHandler, State as EngineState};
use tpt_bastion_ir::{Document, Resource, Value};
use tpt_bastion_providers::abi::{AbiRequest, AbiResponse, PluginServer};
use tpt_bastion_providers::plugin::{
    DataSourceRequest, NativeRuntime, PluginProvider, ProviderPlugin, ProviderRuntime,
    ResourceRequest,
};
use tpt_bastion_providers::registry::PluginRegistry;
use tpt_bastion_providers::scaffold::scaffold_provider;
use tpt_bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};

/// A simple in-process provider used across the tests. It echoes declared
/// attributes and computes a synthetic `id`, remembering nothing between calls.
struct EchoProvider;

impl EchoProvider {
    fn schema_value() -> ProviderSchema {
        ProviderSchema::builder("echo", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("echo_bucket")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: true,
                        update: false,
                        delete: true,
                    })
                    .build(),
            )
            .build()
    }
}

impl ProviderPlugin for EchoProvider {
    fn name(&self) -> &str {
        "echo"
    }
    fn version(&self) -> &str {
        "0.1.0"
    }
    fn schema(&self) -> ProviderSchema {
        Self::schema_value()
    }
    fn create(
        &self,
        req: ResourceRequest,
    ) -> Result<AppliedResource, tpt_bastion_providers::error::ProviderError> {
        let mut attrs = req.attributes.clone();
        attrs.insert("id".to_string(), Value::String(format!("echo::{}", req.id)));
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes: attrs,
        })
    }
    fn read(
        &self,
        req: ResourceRequest,
    ) -> Result<Option<AppliedResource>, tpt_bastion_providers::error::ProviderError> {
        Ok(Some(AppliedResource {
            id: req.id.clone(),
            type_name: req.type_name,
            attributes: req.attributes,
        }))
    }
    fn delete(
        &self,
        _req: ResourceRequest,
    ) -> Result<(), tpt_bastion_providers::error::ProviderError> {
        Ok(())
    }
    fn data_source(
        &self,
        req: DataSourceRequest,
    ) -> Result<Value, tpt_bastion_providers::error::ProviderError> {
        Ok(Value::String(format!("ds:{}", req.name)))
    }
}

fn doc() -> Document {
    let mut attrs = BTreeMap::new();
    attrs.insert("name".to_string(), Value::String("demo".into()));
    Document {
        ir_version: tpt_bastion_ir::IR_VERSION,
        config: None,
        variables: vec![],
        resources: vec![Resource {
            id: "echo_bucket.demo".into(),
            type_name: "echo_bucket".into(),
            name: "demo".into(),
            attributes: attrs,
        }],
        modules: vec![],
        outputs: vec![],
        policies: vec![],
    }
}

#[test]
fn native_runtime_loads_and_serves() {
    let runtime = NativeRuntime::new();
    runtime.register(Arc::new(EchoProvider));

    let plugin = runtime.load("echo").unwrap();
    assert_eq!(plugin.name(), "echo");
    assert_eq!(plugin.schema().resources.len(), 1);

    let applied = plugin
        .create(ResourceRequest {
            id: "echo_bucket.demo".into(),
            type_name: "echo_bucket".into(),
            attributes: std::iter::once(("name".into(), Value::String("demo".into()))).collect(),
        })
        .unwrap();
    assert_eq!(
        applied.attributes.get("id"),
        Some(&Value::String("echo::echo_bucket.demo".into()))
    );

    let ds = plugin
        .data_source(DataSourceRequest {
            name: "amis".into(),
            args: BTreeMap::new(),
        })
        .unwrap();
    assert_eq!(ds, Value::String("ds:amis".into()));
}

#[test]
fn registry_resolves_plugins() {
    let registry = PluginRegistry::new();
    registry.register_runtime("native", Arc::new(NativeRuntime::new()));
    // Must register the plugin *into* the runtime before resolution.
    // (In a real app the runtime is built with its plugins first.)
    let rt = NativeRuntime::new();
    rt.register(Arc::new(EchoProvider));
    registry.register_runtime("native2", Arc::new(rt));

    let plugin = registry.load("echo").unwrap();
    assert_eq!(plugin.name(), "echo");

    let provider = registry.provider("echo").unwrap();
    // The wrapped provider is an engine `Provider`; apply a doc through it.
    let plan = tpt_bastion_engine::plan_document(&doc()).unwrap();
    let exec = Executor::new(provider, Box::new(NoopHandler));
    let state: EngineState = exec.apply(&doc(), &plan, &EngineState::new()).unwrap();
    assert_eq!(state.resources.len(), 1);
}

#[test]
fn plugin_provider_drives_executor_with_rollback() {
    // A provider whose `create` fails for id `echo_bucket.demo` triggers rollback.
    struct FailingProvider;
    impl ProviderPlugin for FailingProvider {
        fn name(&self) -> &str {
            "fail"
        }
        fn version(&self) -> &str {
            "0.1.0"
        }
        fn schema(&self) -> ProviderSchema {
            EchoProvider::schema_value()
        }
        fn create(
            &self,
            _req: ResourceRequest,
        ) -> Result<AppliedResource, tpt_bastion_providers::error::ProviderError> {
            Err(tpt_bastion_providers::error::ProviderError::Rejected {
                provider: "fail".into(),
                message: "boom".into(),
            })
        }
        fn delete(
            &self,
            _req: ResourceRequest,
        ) -> Result<(), tpt_bastion_providers::error::ProviderError> {
            Ok(())
        }
    }

    let runtime = NativeRuntime::new();
    runtime.register(Arc::new(FailingProvider));
    let provider = PluginProvider::load(&runtime, "fail").unwrap();
    let plan = tpt_bastion_engine::plan_document(&doc()).unwrap();
    let exec = Executor::new(provider, Box::new(NoopHandler));
    let result = exec.apply(&doc(), &plan, &EngineState::new());
    assert!(result.is_err());
}

#[test]
fn abi_round_trips_over_in_memory_streams() {
    // Drive the real serve loop over in-memory streams: a client writes requests
    // and reads responses, exercising the same protocol a subprocess uses.
    let schema_json = serde_json::to_string(&AbiRequest::Schema).unwrap();
    let create_json = serde_json::to_string(&AbiRequest::Create(ResourceRequest {
        id: "echo_bucket.demo".into(),
        type_name: "echo_bucket".into(),
        attributes: std::iter::once(("name".into(), Value::String("demo".into()))).collect(),
    }))
    .unwrap();
    let input = format!("{schema_json}\n{create_json}\n");
    let mut output = Vec::new();
    PluginServer::serve(&EchoProvider, input.as_bytes(), &mut output).unwrap();

    let lines: Vec<&str> = std::str::from_utf8(&output).unwrap().lines().collect();
    assert_eq!(lines.len(), 2);
    let resp1: AbiResponse = serde_json::from_str(lines[0]).unwrap();
    assert!(matches!(resp1, AbiResponse::Schema(_)));
    let resp2: AbiResponse = serde_json::from_str(lines[1]).unwrap();
    match resp2 {
        AbiResponse::Applied(a) => {
            assert_eq!(
                a.attributes.get("id"),
                Some(&Value::String("echo::echo_bucket.demo".into()))
            );
        }
        _ => panic!("expected Applied"),
    }
}

#[test]
fn scaffold_emits_a_working_provider_crate() {
    let dir = std::env::temp_dir().join(format!("bastion-scaffold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = scaffold_provider("acme", &dir).unwrap();
    assert!(out.dir.exists());
    assert!(out.files.iter().any(|f| f.ends_with("Cargo.toml")));
    assert!(out.files.iter().any(|f| f.ends_with("src/lib.rs")));
    assert!(out.files.iter().any(|f| f.ends_with("src/main.rs")));

    // The generated lib must reference the SDK and serve over the ABI.
    let lib = std::fs::read_to_string(out.dir.join("src/lib.rs")).unwrap();
    assert!(lib.contains("impl ProviderPlugin for"));
    assert!(lib.contains("fn create"));
    let main = std::fs::read_to_string(out.dir.join("src/main.rs")).unwrap();
    assert!(main.contains("PluginServer::serve"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn conformance_passes_for_echo() {
    tpt_bastion_providers::assert_conforms(&EchoProvider);
}

#[test]
fn conformance_flags_schema_name_mismatch() {
    struct BadProvider;
    impl ProviderPlugin for BadProvider {
        fn name(&self) -> &str {
            "bad"
        }
        fn version(&self) -> &str {
            "0.1.0"
        }
        fn schema(&self) -> ProviderSchema {
            ProviderSchema::builder("other", "0.1.0").build()
        }
        fn create(
            &self,
            req: ResourceRequest,
        ) -> Result<AppliedResource, tpt_bastion_providers::error::ProviderError> {
            Ok(AppliedResource {
                id: req.id,
                type_name: req.type_name,
                attributes: req.attributes,
            })
        }
        fn delete(
            &self,
            _req: ResourceRequest,
        ) -> Result<(), tpt_bastion_providers::error::ProviderError> {
            Ok(())
        }
    }
    let issues = tpt_bastion_providers::check(&BadProvider);
    assert!(issues
        .iter()
        .any(|i| matches!(i.severity, tpt_bastion_providers::Severity::Error)));
}
