// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lower an AST into a typed [`tpt_bastion_ir::Document`].

use crate::ast::{Ast, Block, Expr};
use crate::error::DslError;
use std::collections::BTreeMap;
use tpt_bastion_ir::{
    Config, Document, Module, Output, Policy, PolicyRule, Resource, Value, Variable, IR_VERSION,
};

/// Compile a parsed AST into a Bastion IR [`Document`].
pub fn compile(ast: &crate::ast::Ast) -> Result<Document, DslError> {
    check_references(ast)?;
    let mut doc = Document {
        ir_version: IR_VERSION,
        config: None,
        variables: Vec::new(),
        resources: Vec::new(),
        modules: Vec::new(),
        outputs: Vec::new(),
        policies: Vec::new(),
    };

    let mut seen_resources = std::collections::HashSet::new();

    for block in &ast.blocks {
        match block.kind.as_str() {
            "config" => {
                if doc.config.is_some() {
                    return Err(DslError::Located {
                        line: block.line,
                        col: block.col,
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
                    return Err(DslError::Located {
                        line: block.line,
                        col: block.col,
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
            "policy" => {
                let policy = compile_policy(block)?;
                doc.policies.push(policy);
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

/// Compile a `policy "name" { resource_type, attribute, forbid | require, message }`
/// block. Exactly one of `forbid` and `require` must be given.
fn compile_policy(block: &Block) -> Result<Policy, DslError> {
    let located = |message: String| DslError::Located {
        line: block.line,
        col: block.col,
        message,
    };
    let name = block
        .name
        .clone()
        .ok_or_else(|| located("policy block missing a name".into()))?;
    let attrs = compile_attrs(block)?;
    let text = |key: &str| -> Result<String, DslError> {
        match attrs.get(key) {
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err(located(format!("policy `{name}` needs a string `{key}`"))),
        }
    };
    let resource_type = text("resource_type")?;
    let attribute = text("attribute")?;
    let message = match attrs.get("message") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => {
            return Err(located(format!(
                "policy `{name}`: `message` must be a string"
            )))
        }
        None => None,
    };
    let rule = match (attrs.get("forbid"), attrs.get("require")) {
        (Some(value), None) => PolicyRule::Forbid {
            value: value.clone(),
        },
        (None, Some(value)) => PolicyRule::Require {
            value: value.clone(),
        },
        _ => {
            return Err(located(format!(
                "policy `{name}` needs exactly one of `forbid` or `require`"
            )))
        }
    };
    Ok(Policy {
        name,
        resource_type,
        attribute,
        rule,
        message,
    })
}

/// Every reference in `expr` as `(base, line, col)`, including nested lists and maps.
fn collect_references(expr: &Expr, out: &mut Vec<(String, usize, usize)>) {
    match expr {
        Expr::Reference {
            base, line, col, ..
        } => out.push((base.clone(), *line, *col)),
        Expr::List(items) => items.iter().for_each(|item| collect_references(item, out)),
        Expr::Map(entries) => entries
            .iter()
            .for_each(|(_, item)| collect_references(item, out)),
        _ => {}
    }
}

/// Levenshtein distance between two names.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// The candidate closest to `name`, if it is near enough to be a likely typo.
pub(crate) fn suggest<'a>(
    name: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).clamp(1, 3);
    candidates
        .into_iter()
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// Reject references to names that cannot be resolved, at the position of the
/// reference, so the error appears at compile time rather than at plan time.
///
/// Only resources can be referenced today. A reference to a variable is rejected
/// with a message saying so, because the engine cannot resolve it either.
fn check_references(ast: &Ast) -> Result<(), DslError> {
    let resource_names: Vec<&str> = ast
        .blocks
        .iter()
        .filter(|b| b.kind == "resource")
        .filter_map(|b| b.name.as_deref())
        .collect();
    let variable_names: Vec<&str> = ast
        .blocks
        .iter()
        .filter(|b| b.kind == "variable")
        .filter_map(|b| b.name.as_deref())
        .collect();

    for block in &ast.blocks {
        let owner = match block.kind.as_str() {
            "resource" => format!(
                "`{}.{}`",
                block.type_name.as_deref().unwrap_or("?"),
                block.name.as_deref().unwrap_or("?")
            ),
            "output" => format!("output `{}`", block.name.as_deref().unwrap_or("?")),
            _ => continue,
        };
        let mut refs = Vec::new();
        for (_, value) in &block.attributes {
            collect_references(value, &mut refs);
        }
        for (base, line, col) in refs {
            if resource_names.contains(&base.as_str()) {
                continue;
            }
            let detail = if variable_names.contains(&base.as_str()) {
                "variables cannot be referenced yet".to_string()
            } else {
                match suggest(&base, resource_names.iter().copied()) {
                    Some(near) => format!("no resource has that name; did you mean `{near}`?"),
                    None => "no resource has that name".to_string(),
                }
            };
            return Err(DslError::Located {
                line,
                col,
                message: format!("{owner} references unknown `{base}`: {detail}"),
            });
        }
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
        Expr::Reference { base, path, .. } => Value::Reference {
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
