// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Provider scaffolding — emit a starter provider crate.
//!
//! `scaffold_provider` writes a minimal but *working* provider that implements the
//! [`ProviderPlugin`](crate::plugin::ProviderPlugin) SDK and serves itself over
//! the ABI, ready to be loaded by [`abi::SubprocessRuntime`]. This is the
//! "hello world" starting point described in `docs/provider-guide.md`.

use std::path::{Path, PathBuf};

/// Result of scaffolding: the directory created and the files written.
#[derive(Debug, Clone)]
pub struct ScaffoldOutput {
    /// The crate root that was created.
    pub dir: PathBuf,
    /// Relative paths of every file written.
    pub files: Vec<PathBuf>,
}

/// Generate a starter provider crate named `name` under `out_dir/<name>`.
///
/// The generated crate depends on `tpt-bastion-providers` via a relative path
/// (`../../tpt-bastion-providers`); published providers should switch this to a
/// crates.io version. Returns the created paths on success.
pub fn scaffold_provider(name: &str, out_dir: &Path) -> anyhow::Result<ScaffoldOutput> {
    let safe = sanitize_name(name)?;
    let crate_name = format!("tpt-bastion-provider-{safe}");
    let dir = out_dir.join(&crate_name);
    if dir.exists() {
        anyhow::bail!("refusing to overwrite existing path {}", dir.display());
    }
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("mkdir: {e}"))?;
    std::fs::create_dir_all(dir.join("src")).map_err(|e| anyhow::anyhow!("mkdir src: {e}"))?;

    let pascal = to_pascal_case(&safe);

    let cargo = cargo_toml(&safe, &crate_name);
    let lib = lib_rs(&safe, &pascal);
    let main = main_rs(&pascal);

    let mut files = Vec::new();
    write_file(&dir.join("Cargo.toml"), &cargo, &mut files)?;
    write_file(&dir.join("src").join("lib.rs"), &lib, &mut files)?;
    write_file(&dir.join("src").join("main.rs"), &main, &mut files)?;

    Ok(ScaffoldOutput { dir, files })
}

fn write_file(path: &Path, content: &str, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    std::fs::write(path, content).map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    files.push(path.to_path_buf());
    Ok(())
}

fn cargo_toml(name: &str, crate_name: &str) -> String {
    format!(
        r#"# SPDX-License-Identifier: Apache-2.0 OR MIT

[package]
name = "{crate_name}"
description = "Bastion provider for {name}"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0 OR MIT"

[[bin]]
name = "{name}-provider"
path = "src/main.rs"

[dependencies]
tpt-bastion-providers = {{ path = "../../tpt-bastion-providers" }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#
    )
}

fn lib_rs(name: &str, pascal: &str) -> String {
    format!(
        r#"// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A minimal Bastion provider for `{name}`.
//!
//! This is scaffolding: it echoes back the attributes you declare and computes a
//! synthetic `id`, so you can wire it into `bastion apply` immediately. Replace
//! `create`/`delete` with real API calls and extend `schema()` as you add
//! resource types. See docs/provider-guide.md.

use tpt_bastion_providers::plugin::{{DataSourceRequest, ProviderPlugin, ResourceRequest}};
use tpt_bastion_providers::schema::{{AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType}};
use tpt_bastion_providers::error::ProviderError;
use tpt_bastion_engine::provider::AppliedResource;
use std::collections::BTreeMap;

pub struct {pascal}Provider;

impl {pascal}Provider {{
    pub fn new() -> Self {{ {pascal}Provider }}
}}

impl ProviderPlugin for {pascal}Provider {{
    fn name(&self) -> &str {{ "{name}" }}

    fn version(&self) -> &str {{ "0.1.0" }}

    fn schema(&self) -> ProviderSchema {{
        ProviderSchema::builder("{name}", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("{name}_example")
                    .description("An example resource managed by the {name} provider.")
                    .attribute(AttributeSpec::required("name", ValueType::String))
                    .attribute(AttributeSpec::computed("id", ValueType::String))
                    .lifecycle(LifecycleOps {{ create: true, read: true, update: true, delete: true }})
                    .build(),
            )
            .build()
    }}

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {{
        // TODO: call the real {name} API here, then return the applied state.
        let mut attributes: BTreeMap<String, tpt_bastion_ir::Value> = req.attributes.clone();
        attributes.insert(
            "id".to_string(),
            tpt_bastion_ir::Value::String(format!("{{}}.computed", req.id)),
        );
        Ok(AppliedResource {{
            id: req.id,
            type_name: req.type_name,
            attributes,
        }})
    }}

    fn delete(&self, _req: ResourceRequest) -> Result<(), ProviderError> {{
        // TODO: call the real {name} API to destroy the resource here.
        Ok(())
    }}

    fn data_source(&self, _req: DataSourceRequest) -> Result<tpt_bastion_ir::Value, ProviderError> {{
        // TODO: implement read-only queries (e.g. look up an AMI id).
        Ok(tpt_bastion_ir::Value::Null)
    }}
}}
"#
    )
}

fn main_rs(pascal: &str) -> String {
    format!(
        r#"// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Server entry point: runs the {pascal} provider over the ABI so it can be
//! loaded by `tpt_bastion_providers::abi::SubprocessRuntime` (each provider in its
//! own isolated OS process).

fn main() {{
    let provider = {pascal}Provider::new();
    let _ = tpt_bastion_providers::abi::PluginServer::serve(
        &provider,
        std::io::stdin(),
        std::io::stdout(),
    );
}}
"#
    )
}

/// Reject provider names that could escape the output directory or produce an
/// invalid crate name. Returns a normalized, filesystem-safe identifier.
fn sanitize_name(name: &str) -> anyhow::Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("provider name must not be empty");
    }
    if trimmed.starts_with('-') || trimmed.starts_with('_') {
        anyhow::bail!("provider name must not start with a separator: `{name}`");
    }
    for part in trimmed.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            anyhow::bail!("provider name must not contain path separators or `.`/`..`: `{name}`");
        }
    }
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            anyhow::bail!("provider name contains an invalid character `{ch}`: `{name}`");
        }
    }
    if out.is_empty() {
        anyhow::bail!("provider name must contain at least one alphanumeric character");
    }
    Ok(out)
}

/// Convert a kebab/snake/flat name to PascalCase for the generated type.
fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in name.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push_str("Generated");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_handles_separators() {
        assert_eq!(to_pascal_case("aws-s3"), "AwsS3");
        assert_eq!(to_pascal_case("local_file"), "LocalFile");
        assert_eq!(to_pascal_case(""), "Generated");
    }

    #[test]
    fn sanitize_rejects_traversal_and_junk() {
        for bad in ["", "..", "../x", "a/../b", "-x", "_y", "a b", "a.b", "a!"] {
            assert!(
                sanitize_name(bad).is_err(),
                "expected `{bad}` to be rejected"
            );
        }
        assert_eq!(sanitize_name("aws-s3").unwrap(), "aws-s3");
        assert_eq!(sanitize_name(" My_Prov ").unwrap(), "My_Prov");
    }
}
