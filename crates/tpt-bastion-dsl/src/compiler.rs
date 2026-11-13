// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lower an AST into a typed [`tpt_bastion_ir::Document`].

use crate::ast::{Block, Expr};
use crate::error::DslError;
use std::collections::BTreeMap;
use tpt_bastion_ir::{Config, Document, Module, Output, Resource, Value, Variable, IR_VERSION};

/// Compile a parsed AST into a Bastion IR [`Document`].
pub fn compile(ast: &crate::ast::Ast) -> Result<Document, DslError> {
    let mut doc = Document {
        ir_version: IR_VERSION,
        config: None,
        variables: Vec::new(),
        resources: Vec::new(),
        modules: Vec::new(),
        outputs: Vec::new(),
    };

    let mut seen_resources = std::collections::HashSet::new();

    for block in &ast.blocks {
        match block.kind.as_str() {
            "config" => {
                if doc.config.is_some() {
                    return Err(DslError::Compile {
                        message: "duplicate `config` block".into(),
                    });
                }
                doc.config = Some(compile_config(block)?);
            }
            "resource" => {
                let type_name = block.type_name.clone().ok_or_else(|| DslError::Compile {
                    message: "resource block missing type".into(),
                })?;
                let name = block.name.clone().ok_or_else(|| DslError::Compile {
                    message: "resource block missing name".into(),
                })?;
                let id = format!("{type_name}.{name}");
                if !seen_resources.insert(id.clone()) {
                    return Err(DslError::Compile {
                        message: format!("duplicate resource `{id}`"),
                    });
                }
                doc.resources.push(Resource {
                    id,
                    type_name,
                    name,
                    attributes: compile_attrs(block)?,
                });
            }
            "module" => {
                let name = block.name.clone().ok_or_else(|| DslError::Compile {
                    message: "module block missing name".into(),
                })?;
                let attrs = compile_attrs(block)?;
                let source = attrs.get("source").and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                });
                doc.modules.push(Module {
                    name,
                    source,
                    attributes: attrs,
                });
            }
            "variable" => {
                let name = block.name.clone().ok_or_else(|| DslError::Compile {
                    message: "variable block missing name".into(),
                })?;
                let attrs = compile_attrs(block)?;
                let type_name = attrs.get("type").and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                });
                let default = attrs.get("default").cloned();
                doc.variables.push(Variable {
                    name,
                    type_name,
                    default,
                });
            }
            "output" => {
                let name = block.name.clone().ok_or_else(|| DslError::Compile {
                    message: "output block missing name".into(),
                })?;
                let attrs = compile_attrs(block)?;
                let value = attrs
                    .get("value")
                    .cloned()
                    .ok_or_else(|| DslError::Compile {
                        message: format!("output `{name}` missing `value`"),
                    })?;
                doc.outputs.push(Output { name, value });
            }
            other => {
                return Err(DslError::Compile {
                    message: format!("unsupported block kind `{other}`"),
                });
            }
        }
    }

    check_references(&doc)?;
    Ok(doc)
}

/// Every reference base mentioned in `value`, including nested lists and maps.
fn collect_reference_bases(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Reference { base, .. } => out.push(base.clone()),
        Value::List(items) => items
            .iter()
            .for_each(|item| collect_reference_bases(item, out)),
        Value::Map(map) => map
            .values()
            .for_each(|item| collect_reference_bases(item, out)),
        _ => {}
    }
}

/// Reject references to names that cannot be resolved, so the error appears at
/// compile time rather than at plan time.
///
/// Only resources can be referenced today. A reference to a variable or module
/// is rejected here too, with a message saying so, because the engine cannot
/// resolve it either.
fn check_references(doc: &Document) -> Result<(), DslError> {
    let resource_names: std::collections::HashSet<&str> =
        doc.resources.iter().map(|r| r.name.as_str()).collect();
    let variable_names: std::collections::HashSet<&str> =
        doc.variables.iter().map(|v| v.name.as_str()).collect();

    let check = |owner: &str, value: &Value| -> Result<(), DslError> {
        let mut bases = Vec::new();
        collect_reference_bases(value, &mut bases);
        for base in bases {
            if resource_names.contains(base.as_str()) {
                continue;
            }
            let detail = if variable_names.contains(base.as_str()) {
                "variables cannot be referenced yet".to_string()
            } else {
                "no resource has that name".to_string()
            };
            return Err(DslError::Compile {
                message: format!("{owner} references unknown `{base}`: {detail}"),
            });
        }
        Ok(())
    };

    for resource in &doc.resources {
        for value in resource.attributes.values() {
            check(&format!("`{}`", resource.id), value)?;
        }
    }
    for output in &doc.outputs {
        check(&format!("output `{}`", output.name), &output.value)?;
    }
    Ok(())
}

fn compile_config(block: &Block) -> Result<Config, DslError> {
    let attrs = compile_attrs(block)?;
    let name = attrs.get("name").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let region = attrs.get("region").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        _ => None,
    });
    let mut extra = BTreeMap::new();
    for (k, v) in &attrs {
        if k != "name" && k != "region" {
            extra.insert(k.clone(), v.clone());
        }
    }
    Ok(Config {
        name,
        region,
        extra,
    })
}

fn compile_attrs(block: &Block) -> Result<BTreeMap<String, Value>, DslError> {
    let mut map = BTreeMap::new();
    for (key, expr) in &block.attributes {
        if map.contains_key(key) {
            return Err(DslError::Compile {
                message: format!("duplicate attribute `{key}` in {} block", block.kind),
            });
        }
        map.insert(key.clone(), compile_expr(expr)?);
    }
    Ok(map)
}

fn compile_expr(expr: &Expr) -> Result<Value, DslError> {
    Ok(match expr {
        Expr::String(s) => Value::String(s.clone()),
        Expr::Number(n) => Value::Number(*n),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Null => Value::Null,
        Expr::Reference { base, path } => Value::Reference {
            base: base.clone(),
            path: path.clone(),
        },
        Expr::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(compile_expr(item)?);
            }
            Value::List(out)
        }
        Expr::Map(entries) => {
            let mut out = BTreeMap::new();
            for (k, v) in entries {
                if out.contains_key(k) {
                    return Err(DslError::Compile {
                        message: format!("duplicate key `{k}` in object"),
                    });
                }
                out.insert(k.clone(), compile_expr(v)?);
            }
            Value::Map(out)
        }
    })
}
