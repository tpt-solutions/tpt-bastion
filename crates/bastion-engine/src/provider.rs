// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider abstraction for the execution engine.
//!
//! A [`Provider`] performs the actual infrastructure mutations behind an apply.
//! The engine ships a [`MockProvider`] used for dry-runs and tests; real cloud
//! providers (Phase 6) will implement the same trait behind the Wasm plugin
//! runtime.

use crate::error::EngineError;
use bastion_ir::{Resource, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

    fn destroy(&self, _resource: &AppliedResource) -> Result<(), EngineError> {
        Ok(())
    }
}
