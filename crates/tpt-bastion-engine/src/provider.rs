// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider abstraction for the execution engine.
//!
//! A [`Provider`] performs the actual infrastructure mutations behind an apply.
//! The engine ships a [`MockProvider`] used for dry-runs and tests; real cloud
//! providers implement the same trait through the plugin runtime in
//! `tpt-bastion-providers`. Those cloud providers are still stubs.

use crate::error::EngineError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tpt_bastion_ir::{Resource, Value};

/// The result of successfully applying a single resource.
///
/// Carries the resolved attributes (references evaluated by the provider) and
/// is what gets persisted to state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedResource {
    /// Fully-qualified id, e.g. `aws_s3_bucket.my_bucket`.
    pub id: String,
    /// Provider-scoped type, e.g. `aws_s3_bucket`.
    pub type_name: String,
    /// Resulting attributes (may differ from the declared attributes once the
    /// provider fills in computed values).
    pub attributes: BTreeMap<String, Value>,
}

/// Performs infrastructure mutations for an apply/rollback.
///
/// Implementors are expected to be cheap to share (`&dyn Provider`), so methods
/// take `&self`.
pub trait Provider {
    /// Create the given resource, returning its applied (post-create) state.
    fn create(&self, resource: &Resource) -> Result<AppliedResource, EngineError>;

    /// Update a resource in place so it matches `desired`, returning its new
    /// applied state. Providers that cannot update in place keep this default,
    /// and the executor reports an error instead of silently recreating.
    fn update(
        &self,
        current: &AppliedResource,
        desired: &Resource,
    ) -> Result<AppliedResource, EngineError> {
        let _ = desired;
        Err(EngineError::Provider(format!(
            "updating `{}` in place is not supported by this provider",
            current.id
        )))
    }

    /// Re-read a resource from the real infrastructure. `Ok(None)` means the
    /// resource no longer exists. The default reports the operation as
    /// unsupported, so drift detection never assumes a resource is still there.
    fn read(&self, current: &AppliedResource) -> Result<Option<AppliedResource>, EngineError> {
        Err(EngineError::Provider(format!(
            "reading `{}` is not supported by this provider",
            current.id
        )))
    }

    /// Destroy a previously-applied resource (used during rollback).
    fn destroy(&self, resource: &AppliedResource) -> Result<(), EngineError>;
}

/// A no-op provider used for dry-runs and tests. It records the resource as
/// applied without touching any real infrastructure.
pub struct MockProvider;

impl Provider for MockProvider {
    fn create(&self, resource: &Resource) -> Result<AppliedResource, EngineError> {
        Ok(AppliedResource {
            id: resource.id.clone(),
            type_name: resource.type_name.clone(),
            attributes: resource.attributes.clone(),
        })
    }

    fn update(
        &self,
        _current: &AppliedResource,
        desired: &Resource,
    ) -> Result<AppliedResource, EngineError> {
        self.create(desired)
    }

    fn read(&self, current: &AppliedResource) -> Result<Option<AppliedResource>, EngineError> {
        Ok(Some(current.clone()))
    }

    fn destroy(&self, _resource: &AppliedResource) -> Result<(), EngineError> {
        Ok(())
    }
}

/// Routes each resource to a provider by the prefix of its type name.
///
/// Routes are matched by longest prefix, so `aws_s3_` can override a broader
/// `aws_` route. A resource with no matching route is an error.
#[derive(Default)]
pub struct ProviderRouter {
    routes: Vec<(String, Box<dyn Provider + Send + Sync>)>,
}

impl ProviderRouter {
    /// An empty router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Send every type whose name starts with `prefix` to `provider`.
    pub fn route(
        mut self,
        prefix: impl Into<String>,
        provider: Box<dyn Provider + Send + Sync>,
    ) -> Self {
        self.routes.push((prefix.into(), provider));
        self
    }

    fn provider_for(&self, type_name: &str) -> Result<&(dyn Provider + Send + Sync), EngineError> {
        self.routes
            .iter()
            .filter(|(prefix, _)| type_name.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, provider)| provider.as_ref())
            .ok_or_else(|| {
                EngineError::Provider(format!("no provider is registered for `{type_name}`"))
            })
    }
}

impl Provider for ProviderRouter {
    fn create(&self, resource: &Resource) -> Result<AppliedResource, EngineError> {
        self.provider_for(&resource.type_name)?.create(resource)
    }

    fn update(
        &self,
        current: &AppliedResource,
        desired: &Resource,
    ) -> Result<AppliedResource, EngineError> {
        self.provider_for(&current.type_name)?
            .update(current, desired)
    }

    fn read(&self, current: &AppliedResource) -> Result<Option<AppliedResource>, EngineError> {
        self.provider_for(&current.type_name)?.read(current)
    }

    fn destroy(&self, resource: &AppliedResource) -> Result<(), EngineError> {
        self.provider_for(&resource.type_name)?.destroy(resource)
    }
}
