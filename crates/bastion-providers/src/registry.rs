// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider registry: discover and cache plugins by name.

use crate::error::RuntimeError;
use crate::plugin::{ProviderPlugin, ProviderRuntime};
use std::sync::Arc;
use std::sync::RwLock;

/// A named collection of provider runtimes.
///
/// The registry is how the CLI and long-running agents resolve `provider =
/// "aws"` into a concrete [`ProviderPlugin`]. Runtimes are registered against a
/// namespace; lookups fall through namespaces in registration order.
pub struct PluginRegistry {
    runtimes: RwLock<Vec<(String, Arc<dyn ProviderRuntime>)>>,
}

impl PluginRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        PluginRegistry {
            runtimes: RwLock::new(Vec::new()),
        }
    }

    /// Register a runtime under `namespace` (e.g. `"native"`, `"wasm"`).
    pub fn register_runtime(&self, namespace: &str, runtime: Arc<dyn ProviderRuntime>) {
        if let Ok(mut g) = self.runtimes.write() {
            g.push((namespace.to_string(), runtime));
        }
    }

    /// Resolve a plugin by name, searching every registered runtime.
    pub fn load(&self, name: &str) -> Result<Arc<dyn ProviderPlugin>, RuntimeError> {
        let g = self
            .runtimes
            .read()
            .map_err(|_| RuntimeError::NotFound(name.to_string()))?;
        let mut last_err = RuntimeError::NotFound(name.to_string());
        for (ns, rt) in g.iter() {
            match rt.load(name) {
                Ok(p) => return Ok(p),
                Err(e) => {
                    last_err = RuntimeError::Process {
                        name: format!("{ns}:{name}"),
                        message: e.to_string(),
                    }
                }
            }
        }
        Err(last_err)
    }

    /// All plugin names visible across every runtime.
    pub fn names(&self) -> Vec<String> {
        match self.runtimes.read().ok() {
            Some(guard) => guard.iter().flat_map(|(_ns, rt)| rt.names()).collect(),
            None => Vec::new(),
        }
    }

    /// Build a [`crate::plugin::PluginProvider`] for `name` via this registry.
    pub fn provider(&self, name: &str) -> Result<crate::plugin::PluginProvider, RuntimeError> {
        let plugin = self.load(name)?;
        Ok(crate::plugin::PluginProvider::from_plugin(plugin))
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
