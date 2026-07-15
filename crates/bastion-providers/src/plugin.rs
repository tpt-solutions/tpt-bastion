// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider SDK, runtime abstraction, and engine adapter.
//!
//! [`ProviderPlugin`] is the trait plugin authors implement — the provider SDK.
//! [`ProviderRuntime`] is the host-side abstraction that loads plugins; the crate
//! ships [`NativeRuntime`] (in-process) and, in [`abi`], a subprocess runtime
//! that sandboxes each provider in its own OS process. [`PluginProvider`] adapts
//! any loaded plugin to the engine's [`Provider`](bastion_engine::Provider) trait
//! so the executor can apply plugin-backed infrastructure without caring where the
//! plugin lives.

use crate::error::{ProviderError, RuntimeError};
use crate::schema::ProviderSchema;
use bastion_engine::error::EngineError;
use bastion_engine::provider::{AppliedResource, Provider};
use bastion_ir::Resource as IrResource;
use bastion_ir::Value;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// A request to act on a single resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceRequest {
    /// Fully-qualified id, e.g. `aws_s3_bucket.my_bucket`.
    pub id: String,
    /// Provider-scoped type name, e.g. `aws_s3_bucket`.
    pub type_name: String,
    /// The resource's attributes (resolved references already substituted).
    pub attributes: BTreeMap<String, Value>,
}

impl ResourceRequest {
    /// Build a request from an IR resource.
    pub fn from_resource(r: &IrResource) -> Self {
        ResourceRequest {
            id: r.id.clone(),
            type_name: r.type_name.clone(),
            attributes: r.attributes.clone(),
        }
    }

    /// Build a request from an already-applied resource.
    pub fn from_applied(a: &AppliedResource) -> Self {
        ResourceRequest {
            id: a.id.clone(),
            type_name: a.type_name.clone(),
            attributes: a.attributes.clone(),
        }
    }
}

/// A request to query a read-only data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataSourceRequest {
    /// Data source name.
    pub name: String,
    /// Arguments for the data source query.
    pub args: BTreeMap<String, Value>,
}

/// The provider SDK: the trait every provider implements.
///
/// `read`, `update`, and `data_source` have default implementations that report
/// `UnsupportedOp`, so a minimal provider only needs `name`, `version`,
/// `schema`, `create`, and `delete`.
pub trait ProviderPlugin: Send + Sync + 'static {
    /// Provider name (e.g. `aws`). Must be unique within a registry.
    fn name(&self) -> &str;

    /// Provider version (semver).
    fn version(&self) -> &str;

    /// The capability schema describing this provider's resources/data sources.
    fn schema(&self) -> ProviderSchema;

    /// Create a resource, returning its applied (post-create) state.
    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError>;

    /// Read a resource back. Returns `None` if it no longer exists.
    fn read(&self, _req: ResourceRequest) -> Result<Option<AppliedResource>, ProviderError> {
        Err(ProviderError::UnsupportedOp {
            provider: self.name().to_string(),
            op: "read".to_string(),
        })
    }

    /// Update a resource in place.
    fn update(&self, _req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        Err(ProviderError::UnsupportedOp {
            provider: self.name().to_string(),
            op: "update".to_string(),
        })
    }

    /// Destroy a resource (used during rollback and deletes).
    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError>;

    /// Query a read-only data source.
    fn data_source(&self, _req: DataSourceRequest) -> Result<Value, ProviderError> {
        Err(ProviderError::UnsupportedOp {
            provider: self.name().to_string(),
            op: "data_source".to_string(),
        })
    }
}

/// The host-side abstraction that loads plugins by name.
///
/// Implementors decide *where* a plugin runs: in-process ([`NativeRuntime`]) or
/// as an isolated subprocess ([`crate::abi::SubprocessRuntime`]). The engine
/// only ever sees [`PluginProvider`], so swapping runtimes is a one-line change.
pub trait ProviderRuntime: Send + Sync {
    /// Load a plugin by name, returning a shareable handle.
    fn load(&self, name: &str) -> Result<Arc<dyn ProviderPlugin>, RuntimeError>;

    /// The names of all plugins this runtime can load.
    fn names(&self) -> Vec<String> {
        Vec::new()
    }
}

/// An in-process provider runtime.
///
/// Plugins are registered directly as trait objects, so there is no IPC overhead —
/// ideal for built-in providers, tests, and local development.
pub struct NativeRuntime {
    plugins: RwLock<HashMap<String, Arc<dyn ProviderPlugin>>>,
}

impl NativeRuntime {
    /// An empty runtime.
    pub fn new() -> Self {
        NativeRuntime {
            plugins: RwLock::new(HashMap::new()),
        }
    }

    /// Register a plugin under its [`ProviderPlugin::name`].
    pub fn register(&self, plugin: Arc<dyn ProviderPlugin>) {
        if let Ok(mut g) = self.plugins.write() {
            g.insert(plugin.name().to_string(), plugin);
        }
    }
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRuntime for NativeRuntime {
    fn load(&self, name: &str) -> Result<Arc<dyn ProviderPlugin>, RuntimeError> {
        let g = self
            .plugins
            .read()
            .map_err(|_| RuntimeError::NotFound(name.to_string()))?;
        g.get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::NotFound(name.to_string()))
    }

    fn names(&self) -> Vec<String> {
        self.plugins
            .read()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Adapts a loaded [`ProviderPlugin`] to the engine's [`Provider`] trait.
///
/// Construction resolves the plugin once through a [`ProviderRuntime`]; thereafter
/// every `create`/`destroy` is a cheap in-process (or IPC) call into the plugin.
pub struct PluginProvider {
    plugin: Arc<dyn ProviderPlugin>,
}

impl PluginProvider {
    /// Resolve `name` through `runtime` and wrap the resulting plugin.
    pub fn load(runtime: &dyn ProviderRuntime, name: &str) -> Result<Self, RuntimeError> {
        let plugin = runtime.load(name)?;
        Ok(PluginProvider { plugin })
    }

    /// Wrap an already-resolved plugin handle.
    pub fn from_plugin(plugin: Arc<dyn ProviderPlugin>) -> Self {
        PluginProvider { plugin }
    }
}

impl Provider for PluginProvider {
    fn create(&self, resource: &IrResource) -> Result<AppliedResource, EngineError> {
        let req = ResourceRequest::from_resource(resource);
        self.plugin
            .create(req)
            .map_err(|e| EngineError::Provider(e.to_string()))
    }

    fn destroy(&self, resource: &AppliedResource) -> Result<(), EngineError> {
        let req = ResourceRequest::from_applied(resource);
        self.plugin
            .delete(req)
            .map_err(|e| EngineError::Provider(e.to_string()))
    }
}
