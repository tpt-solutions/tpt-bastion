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
/// The generated crate depends on `bastion-providers` via a relative path
/// (`../../bastion-providers`); published providers should switch this to a
/// crates.io version. Returns the created paths on success.
pub fn scaffold_provider(name: &str, out_dir: &Path) -> anyhow::Result<ScaffoldOutput> {
    let crate_name = format!("bastion-provider-{name}");
    let dir = out_dir.join(&crate_name);
    if dir.exists() {
        anyhow::bail!("refusing to overwrite existing path {}", dir.display());
    }
    std::fs::create_dir_all(&dir).map_err(|e| anyhow::anyhow!("mkdir: {e}"))?;
    std::fs::create_dir_all(dir.join("src")).map_err(|e| anyhow::anyhow!("mkdir src: {e}"))?;

    let pascal = to_pascal_case(name);

    let cargo = cargo_toml(name, &crate_name);
    let lib = lib_rs(name, &pascal);
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
bastion-providers = {{ path = "../../bastion-providers" }}
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

use bastion_providers::plugin::{{DataSourceRequest, ProviderPlugin, ResourceRequest}};
use bastion_providers::schema::{{AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType}};
use bastion_providers::error::ProviderError;
use bastion_engine::provider::AppliedResource;
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
        let mut attributes: BTreeMap<String, bastion_ir::Value> = req.attributes.clone();
        attributes.insert(
            "id".to_string(),
            bastion_ir::Value::String(format!("{{}}.computed", req.id)),
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

    fn data_source(&self, _req: DataSourceRequest) -> Result<bastion_ir::Value, ProviderError> {{
        // TODO: implement read-only queries (e.g. look up an AMI id).
        Ok(bastion_ir::Value::Null)
    }}
}}
"#
    )
}

fn main_rs(pascal: &str) -> String {
    format!(
        r#"// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Server entry point: runs the {pascal} provider over the ABI so it can be
//! loaded by `bastion_providers::abi::SubprocessRuntime` (each provider in its
//! own isolated OS process).

fn main() {{
    let provider = {pascal}Provider::new();
    let _ = bastion_providers::abi::PluginServer::serve(
        &provider,
        std::io::stdin(),
        std::io::stdout(),
    );
}}
"#
    )
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
}
