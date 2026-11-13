// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Terraform / OpenTofu-compatible provider shim for TPT Bastion (Phase 6).
//!
//! This shim lets Bastion drive existing Terraform providers *today* without
//! reimplementing them. For each resource it renders an HCL `resource` block and
//! shells out to `terraform`/`tofu` (`BASTION_TF_BIN`, default `terraform`).
//!
//! # Working directories and state
//!
//! Each Bastion resource gets its own persistent working directory:
//! `$BASTION_TF_STATE_DIR/<resource id>` (default `.bastion/terraform`, relative
//! to the current directory). The directory holds the generated `main.tf` and
//! Terraform's own `terraform.tfstate`. The state must survive between runs,
//! because `delete` can only destroy what Terraform recorded when it created the
//! resource. Runs against one directory are serialized with a per-directory lock.
//!
//! `delete` fails when no Terraform state exists for the resource. It never
//! reports success for a destroy that did not happen.
//!
//! The shim is intentionally permissive: any `type_name` is passed through to
//! HCL, so coverage grows as you install upstream providers. References are not
//! yet translated (HCL needs concrete values); resolve them in the DSL first.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tpt_bastion_engine::provider::AppliedResource;
use tpt_bastion_ir::Value;
use tpt_bastion_providers::error::ProviderError;
use tpt_bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use tpt_bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};

/// Name used in error messages.
const PROVIDER: &str = "terraform-shim";

/// One lock per working directory, so two runs never use the same directory at once.
static DIR_LOCKS: Mutex<Option<HashMap<PathBuf, Arc<Mutex<()>>>>> = Mutex::new(None);

fn internal(message: impl Into<String>) -> ProviderError {
    ProviderError::Internal {
        provider: PROVIDER.into(),
        message: message.into(),
    }
}

/// The per-directory lock for `dir`, creating it on first use.
fn dir_lock(dir: &Path) -> Arc<Mutex<()>> {
    let mut guard = DIR_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    map.entry(dir.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Map a resource id to a safe directory name (`aws_s3_bucket.demo` stays readable).
fn dir_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The Terraform-compatible shim provider.
pub struct TerraformShimProvider;

impl TerraformShimProvider {
    /// Construct the shim provider.
    pub fn new() -> Self {
        TerraformShimProvider
    }

    /// Which binary to drive (`terraform` or `tofu`).
    fn tf_bin() -> String {
        std::env::var("BASTION_TF_BIN").unwrap_or_else(|_| "terraform".to_string())
    }

    /// Root directory holding one working directory per resource.
    fn state_root() -> PathBuf {
        std::env::var_os("BASTION_TF_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".bastion").join("terraform"))
    }

    /// The working directory for one resource.
    fn resource_dir(id: &str) -> PathBuf {
        Self::state_root().join(dir_name(id))
    }

    /// Render a Bastion resource as an HCL `resource` block.
    pub fn to_hcl(req: &ResourceRequest) -> Result<String, ProviderError> {
        let label = req
            .id
            .split_once('.')
            .map(|(_, name)| name)
            .unwrap_or(&req.id);
        let mut body = String::new();
        for (key, value) in &req.attributes {
            match render_hcl_value(value) {
                Some(rendered) => body.push_str(&format!("  {key} = {rendered}\n")),
                None => {
                    return Err(ProviderError::Rejected {
                        provider: PROVIDER.into(),
                        message: format!(
                            "attribute `{key}` holds a reference; resolve it before applying via the shim"
                        ),
                    })
                }
            }
        }
        Ok(format!(
            "resource \"{}\" \"{}\" {{\n{body}}}\n",
            req.type_name, label
        ))
    }

    /// Write `main.tf` into `dir`, replacing any previous version.
    fn write_main_tf(dir: &Path, hcl: &str) -> Result<(), ProviderError> {
        let path = dir.join("main.tf");
        let mut f = std::fs::File::create(&path)
            .map_err(|e| internal(format!("write {}: {e}", path.display())))?;
        writeln!(f, "{hcl}").map_err(|e| internal(format!("write {}: {e}", path.display())))
    }

    /// Run `bin args...` in `dir`, failing on spawn errors and non-zero exits.
    fn run_tf(bin: &str, args: &[&str], dir: &Path) -> Result<(), ProviderError> {
        let status = Command::new(bin)
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| internal(format!("could not run `{bin}` (is it installed?): {e}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(internal(format!(
                "`{bin} {}` failed with {status} in {}",
                args.join(" "),
                dir.display()
            )))
        }
    }

    /// Initialize the working directory once, then run `args`.
    fn run_in_dir(dir: &Path, args: &[&str]) -> Result<(), ProviderError> {
        let bin = Self::tf_bin();
        if !dir.join(".terraform").exists() {
            Self::run_tf(&bin, &["init", "-input=false"], dir)?;
        }
        Self::run_tf(&bin, args, dir)
    }

    fn create_in_dir(req: &ResourceRequest) -> Result<(), ProviderError> {
        let hcl = Self::to_hcl(req)?;
        let dir = Self::resource_dir(&req.id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| internal(format!("create {}: {e}", dir.display())))?;
        let lock = dir_lock(&dir);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        Self::write_main_tf(&dir, &hcl)?;
        Self::run_in_dir(&dir, &["apply", "-auto-approve", "-input=false"])
    }

    fn delete_in_dir(req: &ResourceRequest) -> Result<(), ProviderError> {
        let hcl = Self::to_hcl(req)?;
        let dir = Self::resource_dir(&req.id);
        let lock = dir_lock(&dir);
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

        if !dir.join("terraform.tfstate").exists() {
            return Err(internal(format!(
                "no Terraform state for `{}` in {}; nothing to destroy",
                req.id,
                dir.display()
            )));
        }
        Self::write_main_tf(&dir, &hcl)?;
        Self::run_in_dir(&dir, &["destroy", "-auto-approve", "-input=false"])?;
        // The resource is gone, so its state and working files are no longer needed.
        std::fs::remove_dir_all(&dir)
            .map_err(|e| internal(format!("remove {}: {e}", dir.display())))
    }
}

impl Default for TerraformShimProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for TerraformShimProvider {
    fn name(&self) -> &str {
        PROVIDER
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        // Permissive: every upstream Terraform resource type is accepted. A future
        // gRPC-based implementation will populate this from the provider's schema.
        ProviderSchema::builder(PROVIDER, "0.1.0")
            .resource(
                ResourceTypeSpec::builder("*")
                    .description("Any Terraform/OpenTofu resource type, passed through to HCL.")
                    .attribute(AttributeSpec::optional("__passthrough", ValueType::Any))
                    .lifecycle(LifecycleOps {
                        create: true,
                        read: false,
                        update: true,
                        delete: true,
                    })
                    .build(),
            )
            .build()
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, ProviderError> {
        Self::create_in_dir(&req)?;
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes: req.attributes.clone(),
        })
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError> {
        Self::delete_in_dir(&req)
    }
}

/// Render an IR value as HCL. Returns `None` for references (not translatable).
fn render_hcl_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            Some(format!("\"{escaped}\""))
        }
        Value::Number(n) => Some(format!("{n}")),
        Value::Bool(b) => Some(if *b { "true".into() } else { "false".into() }),
        Value::Null => Some("null".into()),
        Value::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(render_hcl_value)
                .collect::<Option<Vec<_>>>()?;
            Some(format!("[{}]", parts.join(", ")))
        }
        Value::Map(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let r = render_hcl_value(v)?;
                    Some(format!("{k} = {r}"))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(format!("{{{}}}", parts.join(", ")))
        }
        Value::Reference { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req() -> ResourceRequest {
        let mut attributes = BTreeMap::new();
        attributes.insert("bucket".to_string(), Value::String("my-bucket".into()));
        attributes.insert("acl".to_string(), Value::String("private".into()));
        attributes.insert("versioning".to_string(), Value::Bool(true));
        ResourceRequest {
            id: "aws_s3_bucket.demo".into(),
            type_name: "aws_s3_bucket".into(),
            attributes,
        }
    }

    #[test]
    fn renders_hcl_resource() {
        let hcl = TerraformShimProvider::to_hcl(&req()).unwrap();
        assert!(hcl.contains("resource \"aws_s3_bucket\" \"demo\" {"));
        assert!(hcl.contains("bucket = \"my-bucket\""));
        assert!(hcl.contains("versioning = true"));
    }

    #[test]
    fn rejects_references() {
        let mut attributes = BTreeMap::new();
        attributes.insert(
            "bucket".to_string(),
            Value::Reference {
                base: "other".into(),
                path: vec!["bucket".into()],
            },
        );
        let r = ResourceRequest {
            id: "aws_s3_bucket.demo".into(),
            type_name: "aws_s3_bucket".into(),
            attributes,
        };
        assert!(TerraformShimProvider::to_hcl(&r).is_err());
    }

    #[test]
    fn resource_ids_map_to_distinct_safe_dirs() {
        assert_eq!(dir_name("aws_s3_bucket.demo"), "aws_s3_bucket.demo");
        assert_eq!(dir_name("a/b\\c:d"), "a_b_c_d");
        assert_ne!(dir_name("x.a"), dir_name("x.b"));
    }

    #[test]
    fn delete_without_state_is_an_error_not_a_success() {
        let root = std::env::temp_dir().join(format!("bastion-tf-test-{}", std::process::id()));
        std::env::set_var("BASTION_TF_STATE_DIR", &root);
        let result = TerraformShimProvider::new().delete(req());
        std::env::remove_var("BASTION_TF_STATE_DIR");
        let _ = std::fs::remove_dir_all(&root);
        assert!(matches!(result, Err(ProviderError::Internal { .. })));
    }
}
