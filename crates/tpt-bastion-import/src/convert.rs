// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Convert a Terraform configuration file into a Bastion `.bcl` document.
//!
//! The conversion is deliberately conservative. Anything it cannot translate
//! faithfully is left out of the output and recorded as a [`Note`] with its line
//! number. A note never means "approximated": a dropped `count` would change what
//! gets created, so `count`, `for_each`, and similar constructs are reported, not
//! copied.
//!
//! The output is validated with the Bastion compiler before it is returned, so a
//! successful conversion always yields a document that compiles.

use crate::hcl::{self, Attr, Block, Body, HclError, Item, Value};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Meta-arguments and blocks that change how Terraform manages a resource. They
/// have no Bastion equivalent yet, so they are reported and dropped.
const UNSUPPORTED_RESOURCE_ARGS: &[&str] = &[
    "count",
    "for_each",
    "depends_on",
    "provider",
    "lifecycle",
    "provisioner",
    "connection",
];

/// Something in the input that was not converted, or converted with a caveat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// 1-based line in the Terraform source.
    pub line: usize,
    /// What was left out and why.
    pub message: String,
}

/// A successful conversion: the `.bcl` text and what was not converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    /// The Bastion document, formatted.
    pub bcl: String,
    /// Everything that was not converted, in source order.
    pub notes: Vec<Note>,
}

/// Why a conversion failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// The Terraform source is not valid HCL.
    Parse(HclError),
    /// The converted document does not compile. This points at a gap in the
    /// converter; `bcl` is returned so the output can be inspected.
    InvalidOutput {
        /// The compiler's message.
        message: String,
        /// The document that failed to compile.
        bcl: String,
    },
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::Parse(e) => write!(f, "Terraform parse error at {e}"),
            ConvertError::InvalidOutput { message, .. } => {
                write!(f, "the converted document does not compile: {message}")
            }
        }
    }
}

impl std::error::Error for ConvertError {}

/// Convert one Terraform file.
pub fn convert(src: &str) -> Result<Converted, ConvertError> {
    let body = hcl::parse(src).map_err(ConvertError::Parse)?;
    let mut converter = Converter::new(&body);
    let bcl = converter.run(&body);
    let formatted = tpt_bastion_dsl::format::format_source(&bcl);
    if let Err(e) = tpt_bastion_dsl::compile_source(&formatted) {
        return Err(ConvertError::InvalidOutput {
            message: e.to_string(),
            bcl: formatted,
        });
    }
    Ok(Converted {
        bcl: formatted,
        notes: converter.notes,
    })
}

struct Converter {
    /// Resource name to its Terraform type, for rewriting references.
    resource_types: HashMap<String, String>,
    notes: Vec<Note>,
    config: Vec<String>,
    sections: Vec<String>,
}

impl Converter {
    fn new(body: &Body) -> Self {
        let mut resource_types = HashMap::new();
        for item in &body.items {
            if let Item::Block(block) = item {
                if block.kind == "resource" {
                    if let [ty, name] = block.labels.as_slice() {
                        resource_types.insert(name.clone(), ty.clone());
                    }
                }
            }
        }
        Converter {
            resource_types,
            notes: Vec::new(),
            config: Vec::new(),
            sections: Vec::new(),
        }
    }

    fn note(&mut self, line: usize, message: impl Into<String>) {
        self.notes.push(Note {
            line,
            message: message.into(),
        });
    }

    fn run(&mut self, body: &Body) -> String {
        for item in &body.items {
            match item {
                Item::Block(block) => self.top_level(block),
                Item::Attr(attr) => self.note(
                    attr.line,
                    format!(
                        "top-level attribute `{}` is not supported; skipped",
                        attr.name
                    ),
                ),
            }
        }

        let mut out = String::new();
        if !self.config.is_empty() {
            let _ = writeln!(out, "config {{");
            for line in &self.config {
                let _ = writeln!(out, "    {line}");
            }
            let _ = writeln!(out, "}}\n");
        }
        for section in &self.sections {
            out.push_str(section);
            out.push('\n');
        }
        out
    }

    fn top_level(&mut self, block: &Block) {
        match (block.kind.as_str(), block.labels.as_slice()) {
            ("resource", [ty, name]) => self.resource(block, ty, name),
            ("variable", [name]) => self.variable(block, name),
            ("output", [name]) => self.output(block, name),
            ("provider", [name]) => self.provider(block, name),
            ("terraform", []) => self.note(
                block.line,
                "`terraform` settings are not imported; configure backends and providers in Bastion",
            ),
            (kind, _) => self.note(
                block.line,
                format!("`{kind}` blocks are not supported yet; skipped"),
            ),
        }
    }

    fn resource(&mut self, block: &Block, ty: &str, name: &str) {
        // `count` and `for_each` create several resources from one block. Emitting
        // one resource would describe something different from what is created, so
        // the whole block is skipped and reported.
        let expansion = block.body.items.iter().find_map(|item| match item {
            Item::Attr(attr) if attr.name == "count" || attr.name == "for_each" => Some(attr),
            _ => None,
        });
        if let Some(attr) = expansion {
            self.note(
                attr.line,
                format!(
                    "`{}` creates several resources from one block, which is not supported yet; `{ty}.{name}` is skipped",
                    attr.name
                ),
            );
            return;
        }
        if !is_identifier(ty) || !is_identifier(name) {
            self.note(
                block.line,
                format!("resource `{ty}.{name}` has a name that is not a valid Bastion identifier; skipped"),
            );
            return;
        }
        let mut text = format!("resource {ty} {name} {{\n");
        self.body_lines(&block.body, 1, &mut text, true);
        text.push('}');
        self.sections.push(text);
    }

    fn variable(&mut self, block: &Block, name: &str) {
        if !is_identifier(name) {
            self.note(
                block.line,
                format!("variable `{name}` is not a valid identifier; skipped"),
            );
            return;
        }
        let mut text = format!("variable {name} {{\n");
        for item in &block.body.items {
            match item {
                Item::Attr(attr) => match attr.name.as_str() {
                    "default" => self.emit_attr(attr, 1, &mut text),
                    "type" => {
                        // `type = string` is a bare name; keep it as a string label.
                        if let Value::Ref(path) = &attr.value {
                            if let [kind] = path.as_slice() {
                                let _ = writeln!(text, "    type = \"{kind}\"");
                                continue;
                            }
                        }
                        self.note(
                            attr.line,
                            "complex variable types are not supported; type dropped",
                        );
                    }
                    "description" | "sensitive" | "nullable" | "validation" => self.note(
                        attr.line,
                        format!("variable `{}` is not supported; dropped", attr.name),
                    ),
                    other => self.note(
                        attr.line,
                        format!("variable argument `{other}` is not supported; dropped"),
                    ),
                },
                Item::Block(inner) => self.note(
                    inner.line,
                    format!(
                        "nested `{}` block in a variable is not supported; skipped",
                        inner.kind
                    ),
                ),
            }
        }
        text.push('}');
        self.sections.push(text);
    }

    fn output(&mut self, block: &Block, name: &str) {
        if !is_identifier(name) {
            self.note(
                block.line,
                format!("output `{name}` is not a valid identifier; skipped"),
            );
            return;
        }
        let mut text = format!("output {name} {{\n");
        let mut has_value = false;
        for item in &block.body.items {
            match item {
                Item::Attr(attr) if attr.name == "value" => {
                    has_value = true;
                    self.emit_attr(attr, 1, &mut text);
                }
                Item::Attr(attr) => self.note(
                    attr.line,
                    format!("output argument `{}` is not supported; dropped", attr.name),
                ),
                Item::Block(inner) => self.note(
                    inner.line,
                    format!(
                        "nested `{}` block in an output is not supported; skipped",
                        inner.kind
                    ),
                ),
            }
        }
        if !has_value {
            self.note(block.line, format!("output `{name}` has no value; skipped"));
            return;
        }
        text.push('}');
        self.sections.push(text);
    }

    fn provider(&mut self, block: &Block, name: &str) {
        if name != "aws" {
            self.note(
                block.line,
                format!("provider `{name}` is not imported; configure it in Bastion"),
            );
            return;
        }
        for item in &block.body.items {
            match item {
                Item::Attr(attr) if attr.name == "region" => {
                    if let Value::Str(region) = &attr.value {
                        self.config.push(format!("region = {}", quote(region)));
                    } else {
                        self.note(
                            attr.line,
                            "provider region must be a literal string; dropped",
                        );
                    }
                }
                Item::Attr(attr) => self.note(
                    attr.line,
                    format!(
                        "aws provider argument `{}` is not supported; dropped",
                        attr.name
                    ),
                ),
                Item::Block(inner) => self.note(
                    inner.line,
                    format!(
                        "nested `{}` block in the provider is not supported; skipped",
                        inner.kind
                    ),
                ),
            }
        }
    }

    /// Emit the attributes and nested blocks of a resource or nested body.
    fn body_lines(&mut self, body: &Body, depth: usize, out: &mut String, top: bool) {
        for item in &body.items {
            match item {
                Item::Attr(attr) => {
                    if top && UNSUPPORTED_RESOURCE_ARGS.contains(&attr.name.as_str()) {
                        self.note(
                            attr.line,
                            format!(
                                "`{}` changes how the resource is created and is not supported yet; the resource is imported without it",
                                attr.name
                            ),
                        );
                        continue;
                    }
                    self.emit_attr(attr, depth, out);
                }
                Item::Block(inner) => {
                    if top && UNSUPPORTED_RESOURCE_ARGS.contains(&inner.kind.as_str()) {
                        self.note(
                            inner.line,
                            format!(
                                "`{}` blocks are not supported yet; the resource is imported without it",
                                inner.kind
                            ),
                        );
                        continue;
                    }
                    if !inner.labels.is_empty() {
                        self.note(
                            inner.line,
                            format!(
                                "labelled `{}` blocks are not supported; skipped",
                                inner.kind
                            ),
                        );
                        continue;
                    }
                    let pad = "    ".repeat(depth);
                    let _ = writeln!(out, "{pad}{} {{", inner.kind);
                    self.body_lines(&inner.body, depth + 1, out, false);
                    let _ = writeln!(out, "{pad}}}");
                }
            }
        }
    }

    fn emit_attr(&mut self, attr: &Attr, depth: usize, out: &mut String) {
        let pad = "    ".repeat(depth);
        if let Some(text) = self.value(&attr.value, attr.line) {
            let _ = writeln!(out, "{pad}{} = {text}", attr.name);
        }
    }

    /// Translate a value, or record why it cannot be translated and return `None`.
    fn value(&mut self, value: &Value, line: usize) -> Option<String> {
        match value {
            Value::Str(s) => Some(quote(s)),
            Value::Template(_) => {
                self.note(
                    line,
                    "string interpolation (`${...}`) is not supported; the attribute is skipped",
                );
                None
            }
            Value::Num(n) => {
                if *n < 0.0 {
                    self.note(
                        line,
                        "negative numbers are not supported yet; the attribute is skipped",
                    );
                    None
                } else {
                    Some(format!("{n}"))
                }
            }
            Value::Bool(b) => Some(b.to_string()),
            Value::Null => Some("null".to_string()),
            Value::List(items) => {
                let mut parts = Vec::new();
                for item in items {
                    parts.push(self.value(item, line)?);
                }
                Some(format!("[{}]", parts.join(", ")))
            }
            Value::Object(entries) => {
                let mut parts = Vec::new();
                for (key, item) in entries {
                    if !is_identifier(key) {
                        self.note(line, format!("object key `{key}` is not a valid identifier; the attribute is skipped"));
                        return None;
                    }
                    parts.push(format!("{key} = {}", self.value(item, line)?));
                }
                Some(format!("{{ {} }}", parts.join(", ")))
            }
            Value::Ref(path) => self.reference(path, line),
            Value::Call(name) => {
                self.note(
                    line,
                    format!("function `{name}()` is not supported; the attribute is skipped"),
                );
                None
            }
            Value::Other(what) => {
                self.note(
                    line,
                    format!("{what} is not supported; the attribute is skipped"),
                );
                None
            }
        }
    }

    /// `aws_s3_bucket.b.bucket` becomes `b.bucket` when `b` is a resource of that type.
    fn reference(&mut self, path: &[String], line: usize) -> Option<String> {
        let text = path.join(".");
        match path {
            [first, ..] if first == "var" => {
                self.note(
                    line,
                    "variable references (`var.*`) are not supported yet; the attribute is skipped",
                );
                None
            }
            [ty, name, rest @ ..] if self.resource_types.get(name) == Some(ty) => {
                let mut out = name.clone();
                for segment in rest {
                    out.push('.');
                    out.push_str(segment);
                }
                Some(out)
            }
            _ => {
                self.note(
                    line,
                    format!("reference `{text}` does not name a resource in this file; the attribute is skipped"),
                );
                None
            }
        }
    }
}

/// Quote a string for the Bastion DSL, escaping what its lexer understands.
fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Whether `s` can be used as a Bastion identifier.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_resources_and_rewrites_references() {
        let tf = "resource \"aws_s3_bucket\" \"a\" {\n  bucket = \"first\"\n}\nresource \"aws_s3_bucket\" \"b\" {\n  bucket = \"second\"\n  acl = aws_s3_bucket.a.bucket\n}\n";
        let out = convert(tf).unwrap();
        assert!(out.bcl.contains("resource aws_s3_bucket a {"));
        assert!(out.bcl.contains("acl = a.bucket"), "{}", out.bcl);
        assert!(out.notes.is_empty());
    }

    #[test]
    fn skips_a_count_resource_and_says_so() {
        let tf = "resource \"aws_s3_bucket\" \"a\" {\n  count = 3\n  bucket = \"x\"\n}\n";
        let out = convert(tf).unwrap();
        assert!(!out.bcl.contains("aws_s3_bucket a"), "{}", out.bcl);
        assert_eq!(out.notes.len(), 1);
        assert_eq!(out.notes[0].line, 2);
        assert!(out.notes[0].message.contains("count"));
        assert!(out.notes[0].message.contains("skipped"));
    }

    #[test]
    fn reports_variable_and_interpolation_references() {
        let tf =
            "resource \"aws_s3_bucket\" \"a\" {\n  bucket = \"x-${var.env}\"\n  acl = var.acl\n}\n";
        let out = convert(tf).unwrap();
        assert_eq!(out.notes.len(), 2);
        assert!(out.notes[0].message.contains("interpolation"));
        assert!(out.notes[1].message.contains("variable"));
    }

    #[test]
    fn converts_variables_outputs_and_the_aws_region() {
        let tf = "provider \"aws\" {\n  region = \"ap-southeast-2\"\n}\nvariable \"name\" {\n  type = string\n  default = \"demo\"\n  description = \"a name\"\n}\nresource \"aws_s3_bucket\" \"b\" {\n  bucket = \"x\"\n}\noutput \"id\" {\n  value = aws_s3_bucket.b.bucket\n}\n";
        let out = convert(tf).unwrap();
        assert!(
            out.bcl.contains("region = \"ap-southeast-2\""),
            "{}",
            out.bcl
        );
        assert!(out.bcl.contains("variable name {"));
        assert!(out.bcl.contains("type = \"string\""));
        assert!(out.bcl.contains("output id {"));
        assert!(out.bcl.contains("value = b.bucket"));
        // The description is dropped, and the user is told.
        assert!(out.notes.iter().any(|n| n.message.contains("description")));
    }

    #[test]
    fn nested_blocks_become_nested_bodies() {
        let tf = "resource \"aws_s3_bucket\" \"a\" {\n  bucket = \"x\"\n  versioning {\n    enabled = true\n  }\n  tags = { env = \"dev\" }\n}\n";
        let out = convert(tf).unwrap();
        assert!(out.bcl.contains("versioning {"), "{}", out.bcl);
        assert!(out.bcl.contains("enabled = true"));
        assert!(out.bcl.contains("tags = { env = \"dev\" }"));
    }

    #[test]
    fn invalid_hcl_is_a_parse_error_with_a_position() {
        let err = convert("resource \"a\" \"b\" {\n  bucket = \n}\n").unwrap_err();
        assert!(matches!(err, ConvertError::Parse(HclError { line: 2, .. })));
    }
}
