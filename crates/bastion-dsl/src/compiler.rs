// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lower an AST into a typed [`bastion_ir::Document`].

use crate::ast::{Block, Expr};
use crate::error::DslError;
use bastion_ir::{Config, Document, Module, Output, Resource, Value, Variable, IR_VERSION};
use std::collections::BTreeMap;

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

    Ok(doc)
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
