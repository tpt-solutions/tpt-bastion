// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Terraform / OpenTofu-compatible provider shim for TPT Bastion (Phase 6).
//!
//! This shim lets Bastion drive existing Terraform providers *today* without
//! reimplementing them. For each resource it renders an HCL `resource` block and
//! shells out to `terraform`/`tofu` (`BASTION_TF_BIN`, default `terraform`) to
//! apply it. This is the pragmatic interop path; the long-term path is to speak
//! the Terraform plugin gRPC protocol directly (see `terraform_plugin_protocol`
//! below), which removes the CLI dependency and lets Bastion consume a
//! provider's schema natively.
//!
//! The shim is intentionally permissive: any `type_name` is passed through to
//! HCL, so coverage grows as you install upstream providers. References are not
//! yet translated (HCL needs concrete values); resolve them in the DSL first.

use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use bastion_providers::error::ProviderError;
use bastion_providers::plugin::{ProviderPlugin, ResourceRequest};
use bastion_providers::schema::{
    AttributeSpec, LifecycleOps, ProviderSchema, ResourceTypeSpec, ValueType,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

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
                        provider: "terraform-shim".into(),
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

    fn apply_via_cli(req: &ResourceRequest) -> Result<(), ProviderError> {
        let hcl = Self::to_hcl(req)?;
        let dir = std::env::temp_dir().join(format!("bastion-tf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| ProviderError::Internal {
            provider: "terraform-shim".into(),
            message: format!("create temp dir: {e}"),
        })?;
        let tf_path: PathBuf = dir.join("main.tf");
        let mut f = std::fs::File::create(&tf_path).map_err(|e| ProviderError::Internal {
            provider: "terraform-shim".into(),
            message: format!("write main.tf: {e}"),
        })?;
        writeln!(f, "{hcl}").map_err(|e| ProviderError::Internal {
            provider: "terraform-shim".into(),
            message: format!("write main.tf: {e}"),
        })?;
        drop(f);

        let bin = Self::tf_bin();
        let init = Command::new(&bin)
            .args(["init", "-input=false"])
            .current_dir(&dir)
            .status();
        let apply = Command::new(&bin)
            .args(["apply", "-auto-approve", "-input=false"])
            .current_dir(&dir)
            .status();

        let _ = std::fs::remove_dir_all(&dir);

        match (init, apply) {
            (Ok(i), Ok(a)) if i.success() && a.success() => Ok(()),
            (i, a) => Err(ProviderError::Internal {
                provider: "terraform-shim".into(),
                message: format!(
                    "{bin} exited (init ok={}, apply ok={}); is it installed?",
                    i.map(|s| s.success()).unwrap_or(false),
                    a.map(|s| s.success()).unwrap_or(false)
                ),
            }),
        }
    }
}

impl Default for TerraformShimProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderPlugin for TerraformShimProvider {
    fn name(&self) -> &str {
        "terraform-shim"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> ProviderSchema {
        // Permissive: every upstream Terraform resource type is accepted. A future
        // gRPC-based implementation will populate this from the provider's schema.
        ProviderSchema::builder("terraform-shim", "0.1.0")
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
        Self::apply_via_cli(&req)?;
        Ok(AppliedResource {
            id: req.id,
            type_name: req.type_name,
            attributes: req.attributes.clone(),
        })
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), ProviderError> {
        let hcl = Self::to_hcl(&req)?;
        let dir = std::env::temp_dir().join(format!("bastion-tf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let tf_path = dir.join("main.tf");
        if let Ok(mut f) = std::fs::File::create(&tf_path) {
            let _ = writeln!(f, "{hcl}");
        }
        let bin = Self::tf_bin();
        let status = Command::new(&bin)
            .args(["destroy", "-auto-approve", "-input=false"])
            .current_dir(&dir)
            .status();
        let _ = std::fs::remove_dir_all(&dir);
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => Err(ProviderError::Internal {
                provider: "terraform-shim".into(),
                message: format!("{bin} destroy failed; is it installed?"),
            }),
        }
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
}
