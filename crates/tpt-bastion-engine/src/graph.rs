// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dependency-graph construction and topological ordering from IR.
//!
//! The graph is built purely from the structured [`tpt_bastion_ir::Value::Reference`]
//! values the compiler preserves: a resource (or output) that references
//! another resource must be created/acted on *after* that resource.

use crate::error::EngineError;
use std::collections::{BTreeMap, HashMap};
use tpt_bastion_ir::{Document, Value};

/// Prefix used for output nodes so they never collide with resource ids.
const OUTPUT_PREFIX: &str = "output.";

/// A dependency graph over a compiled document.
///
/// Nodes are resource ids (`aws_s3_bucket.my_bucket`) and output nodes
/// (`output.bucket_name`). For every node, [`DepGraph::deps`] records the nodes
/// it depends on and therefore must be ordered after.
#[derive(Debug, Clone)]
pub struct DepGraph {
    /// All node ids in the graph.
    pub nodes: Vec<String>,
    /// `deps[node]` = nodes that `node` depends on (created before `node`).
    pub deps: HashMap<String, Vec<String>>,
}

impl DepGraph {
    /// Construct the dependency graph for `doc`.
    ///
    /// Every resource becomes a node, and every `output` becomes a node. A
    /// reference to a resource name creates an edge from that resource to the
    /// referencing node. References to unknown objects are a compile-time-style
    /// error surfaced by the engine.
    pub fn build(doc: &Document) -> Result<DepGraph, EngineError> {
        let mut name_to_id: HashMap<&str, &str> = HashMap::new();
        for r in &doc.resources {
            // Local name maps to its fully-qualified id. Later collisions are
            // not yet an error (would require disambiguation rules), but the
            // first mapping wins for dependency resolution.
            name_to_id.entry(r.name.as_str()).or_insert(r.id.as_str());
        }

        let mut nodes: Vec<String> = doc.resources.iter().map(|r| r.id.clone()).collect();
        for o in &doc.outputs {
            nodes.push(format!("{OUTPUT_PREFIX}{}", o.name));
        }
        if nodes.is_empty() {
            return Ok(DepGraph {
                nodes,
                deps: HashMap::new(),
            });
        }

        let mut deps: HashMap<String, Vec<String>> =
            nodes.iter().map(|n| (n.clone(), Vec::new())).collect();

        let mut connect =
            |node: &str, base: &str, known: &HashMap<&str, &str>| -> Result<(), EngineError> {
                match known.get(base) {
                    Some(&target) if target != node => {
                        let entry = deps.get_mut(node).expect("node registered");
                        if !entry.contains(&target.to_string()) {
                            entry.push(target.to_string());
                        }
                        Ok(())
                    }
                    Some(_) => Ok(()), // self-reference, ignore
                    None => Err(EngineError::UnknownReference(base.to_string())),
                }
            };

        for r in &doc.resources {
            for (base, _) in references_in_attrs(&r.attributes) {
                connect(&r.id, &base, &name_to_id)?;
            }
        }
        for o in &doc.outputs {
            let node = format!("{OUTPUT_PREFIX}{}", o.name);
            for (base, _) in references_in_value(&o.value) {
                connect(&node, &base, &name_to_id)?;
            }
        }

        Ok(DepGraph { nodes, deps })
    }

    /// Assign each node a level equal to its longest dependency chain depth.
    ///
    /// Nodes with no dependencies are level `0`; a node's level is
    /// `1 + max(level of its dependencies)`. Two nodes at the same level have
    /// no dependency relationship and can be applied in parallel.
    pub fn levels(&self) -> HashMap<String, usize> {
        let mut memo: HashMap<String, usize> = HashMap::new();
        for n in &self.nodes {
            Self::compute_level(n, &self.deps, &mut memo);
        }
        memo
    }

    /// Recursively compute (and memoize) the dependency level of `node`.
    fn compute_level(
        node: &str,
        deps: &HashMap<String, Vec<String>>,
        memo: &mut HashMap<String, usize>,
    ) -> usize {
        if let Some(&cached) = memo.get(node) {
            return cached;
        }
        // Guard against cycles by marking in-progress with a temporary large
        // value; the cycle is ultimately reported by `order`.
        memo.insert(node.to_string(), usize::MAX);
        let level = deps
            .get(node)
            .map(|d| {
                d.iter()
                    .map(|dep| Self::compute_level(dep, deps, memo))
                    .max()
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(0);
        memo.insert(node.to_string(), level);
        level
    }

    /// Return the node ids in dependency order (dependencies first).
    ///
    /// Uses Kahn's algorithm; a cycle is reported with the offending path.
    pub fn order(&self) -> Result<Vec<String>, EngineError> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Reverse adjacency: dependents[target] = nodes that depend on target.
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        let mut indegree: HashMap<String, usize> = HashMap::new();
        for n in &self.nodes {
            indegree.insert(n.clone(), self.deps.get(n).map(Vec::len).unwrap_or(0));
            for dep in self.deps.get(n).unwrap_or(&Vec::new()) {
                if self.deps.contains_key(dep) {
                    dependents.entry(dep.clone()).or_default().push(n.clone());
                }
            }
        }

        // Seed the queue with dependency-free nodes, keeping a stable order.
        let mut queue: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| indegree.get(*n) == Some(&0))
            .cloned()
            .collect();
        queue.sort_unstable();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while let Some(n) = queue.pop() {
            ordered.push(n.clone());
            let mut next: Vec<String> = dependents.get(&n).cloned().unwrap_or_default();
            next.sort_unstable();
            for dep_node in next {
                let d = indegree.get_mut(&dep_node).expect("known node");
                *d -= 1;
                if *d == 0 {
                    queue.push(dep_node);
                }
            }
            // Keep the queue sorted so output order is deterministic.
            queue.sort_unstable();
        }

        if ordered.len() != self.nodes.len() {
            return Err(EngineError::Cycle {
                path: self.find_cycle(),
            });
        }
        Ok(ordered)
    }

    /// Recover one cycle path from the graph (used for diagnostics).
    fn find_cycle(&self) -> Vec<String> {
        // White/gray/black DFS; reconstruct the cycle when we hit gray.
        #[derive(Copy, Clone)]
        enum Color {
            White,
            Gray,
            Black,
        }
        let mut color: HashMap<String, Color> = self
            .nodes
            .iter()
            .map(|n| (n.clone(), Color::White))
            .collect();
        let empty: Vec<String> = Vec::new();
        let mut stack: Vec<String> = Vec::new();

        fn dfs(
            node: &str,
            graph: &DepGraph,
            color: &mut HashMap<String, Color>,
            stack: &mut Vec<String>,
            empty: &Vec<String>,
        ) -> Option<Vec<String>> {
            color.insert(node.to_string(), Color::Gray);
            stack.push(node.to_string());
            for dep in graph.deps.get(node).unwrap_or(empty) {
                match color.get(dep).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        // Found a back-edge: build cycle from stack.
                        if let Some(pos) = stack.iter().position(|s| s == dep) {
                            return Some(stack[pos..].to_vec());
                        }
                        return Some(stack.clone());
                    }
                    Color::White => {
                        if let Some(cycle) = dfs(dep, graph, color, stack, empty) {
                            return Some(cycle);
                        }
                    }
                    Color::Black => {}
                }
            }
            stack.pop();
            color.insert(node.to_string(), Color::Black);
            None
        }

        for n in &self.nodes {
            if matches!(color.get(n), None | Some(Color::White)) {
                if let Some(cycle) = dfs(n, self, &mut color, &mut stack, &empty) {
                    return cycle;
                }
            }
        }
        Vec::new()
    }
}

/// Collect `(base, path)` references found anywhere in a value.
fn references_in_value(value: &Value) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    match value {
        Value::Reference { base, path } => out.push((base.clone(), path.clone())),
        Value::List(items) => {
            for item in items {
                out.extend(references_in_value(item));
            }
        }
        Value::Map(map) => {
            for v in map.values() {
                out.extend(references_in_value(v));
            }
        }
        _ => {}
    }
    out
}

/// Collect references across an attribute map.
fn references_in_attrs(attrs: &BTreeMap<String, Value>) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for v in attrs.values() {
        out.extend(references_in_value(v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tpt_bastion_ir::{Resource, Value};

    fn resource(id: &str, name: &str, refs: &[&str]) -> Resource {
        let mut attrs = BTreeMap::new();
        for r in refs {
            attrs.insert(
                (*r).to_string(),
                Value::Reference {
                    base: (*r).to_string(),
                    path: vec!["id".to_string()],
                },
            );
        }
        Resource {
            id: id.to_string(),
            type_name: "t".to_string(),
            name: name.to_string(),
            attributes: attrs,
        }
    }

    #[test]
    fn orders_dependencies_before_dependents() {
        let doc = Document {
            ir_version: tpt_bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![
                resource("t.b", "b", &[]),
                resource("t.a", "a", &["b"]),
                resource("t.c", "c", &["a"]),
            ],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
        };
        let g = DepGraph::build(&doc).unwrap();
        let order = g.order().unwrap();
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("t.b") < pos("t.a"));
        assert!(pos("t.a") < pos("t.c"));
    }

    #[test]
    fn detects_cycle() {
        let doc = Document {
            ir_version: tpt_bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![resource("t.a", "a", &["b"]), resource("t.b", "b", &["a"])],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
        };
        let g = DepGraph::build(&doc).unwrap();
        assert!(matches!(g.order(), Err(EngineError::Cycle { .. })));
    }

    #[test]
    fn rejects_unknown_reference() {
        let doc = Document {
            ir_version: tpt_bastion_ir::IR_VERSION,
            config: None,
            variables: vec![],
            resources: vec![resource("t.a", "a", &["ghost"])],
            modules: vec![],
            outputs: vec![],
            policies: vec![],
        };
        let g = DepGraph::build(&doc);
        assert!(matches!(g, Err(EngineError::UnknownReference(_))));
    }
}
