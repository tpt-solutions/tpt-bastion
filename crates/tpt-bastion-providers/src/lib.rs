// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # tpt-bastion-providers
//!
//! The provider plugin system for [TPT Bastion][bastion], implementing
//! **Phase 5** of the roadmap:
//!
//! - **Provider SDK / ABI** — [`plugin::ProviderPlugin`] is the trait plugin
//!   authors implement (resource CRUD, data sources, schema). The host↔plugin
//!   boundary is a stable, serializable protocol ([`abi`]).
//! - **Capability / schema format** — [`schema::ProviderSchema`] describes, in
//!   data, every resource type, its attributes, and supported lifecycle ops, so
//!   tooling can validate and document a provider without executing it.
//! - **Pluggable runtime** — [`plugin::ProviderRuntime`] is the interface a host
//!   implements to load plugins. The crate ships [`plugin::NativeRuntime`]
//!   (in-process, ideal for tests and built-ins) and [`abi::SubprocessRuntime`]
//!   (each provider runs as an isolated OS process, confined on Unix with
//!   resource limits and a read-only [`sandbox`]).
//! - **Sandboxing** — [`sandbox`] confines subprocess providers (resource
//!   limits + read-only `landlock` on Linux) so a misbehaving provider cannot
//!   exhaust the host or tamper with state/credentials.
//! - **Engine integration** — [`plugin::PluginProvider`] adapts any loaded
//!   plugin to the engine's [`Provider`](tpt_bastion_engine::Provider) trait, so the
//!   executor applies plugin-backed infrastructure transparently.
//! - **Registry** — [`registry::PluginRegistry`] discovers and caches plugins by
//!   name for the CLI and long-running agents.
//! - **Scaffolding** — [`scaffold::scaffold_provider`] emits a starter
//!   provider crate, and `docs/provider-guide.md` is the developer guide.
//!
//! [bastion]: https://bastion.tpt.dev

pub mod abi;
pub mod conformance;
pub mod error;
pub mod example;
pub mod plugin;
pub mod registry;
pub mod sandbox;
pub mod scaffold;
pub mod schema;

pub use example::NullProvider;

pub use abi::{AbiRequest, AbiResponse, PluginServer, SubprocessRuntime};
pub use conformance::{assert_conforms, check, ConformanceIssue, Severity};
pub use error::{ProviderError, RuntimeError};
pub use plugin::{
    DataSourceRequest, NativeRuntime, PluginProvider, ProviderPlugin, ProviderRuntime,
    ResourceRequest,
};
pub use registry::PluginRegistry;
pub use schema::{AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType};
