// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A Language Server Protocol server for Bastion `.bcl` documents.
//!
//! The server speaks JSON-RPC over stdio with `Content-Length` framing. It
//! supports full-document sync and offers:
//!
//! - **Diagnostics** from the compiler, at the position of each error.
//! - **Formatting** with the canonical formatter (`format_source`).
//! - **Hover** on a resource type (its schema) or a resource name.
//! - **Go to definition** for a reference, jumping to the resource block.
//! - **Completion** of resource types after `resource`, and of resource names
//!   elsewhere.
//!
//! Positions follow LSP: zero-based lines, and characters counted as Unicode
//! scalar values (exact for the ASCII that `.bcl` identifiers use).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use tpt_bastion_dsl::ast::{Ast, Block};
use tpt_bastion_dsl::error::DslError;
use tpt_bastion_dsl::format::format_source;
use tpt_bastion_dsl::{compile_source, parse, tokenize};
use tpt_bastion_providers::schema::ProviderSchema;

/// LSP `DiagnosticSeverity.Error`.
const SEVERITY_ERROR: u32 = 1;
/// LSP `CompletionItemKind` values used here.
const KIND_VARIABLE: u32 = 6;
const KIND_CLASS: u32 = 7;
/// JSON-RPC error code for an unknown method.
const METHOD_NOT_FOUND: i64 = -32601;

/// Run the server on the process's stdin and stdout until `exit`.
///
/// `schemas` are the provider schemas used for resource-type completion and hover.
pub fn run(schemas: Vec<ProviderSchema>) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut server = Server::new(schemas);
    serve(&mut stdin.lock(), &mut stdout.lock(), &mut server)
}

/// Read messages from `input`, answer them, and write responses to `output`.
/// Returns when the client sends `exit` or closes the stream.
pub fn serve<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    server: &mut Server,
) -> io::Result<()> {
    while let Some(message) = read_message(input)? {
        for reply in server.handle(&message) {
            write_message(output, &reply)?;
        }
        if server.exited {
            break;
        }
    }
    Ok(())
}

/// Read one framed JSON-RPC message, or `None` at end of stream.
pub fn read_message<R: BufRead>(input: &mut R) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok();
        }
    }
    let length = length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write one framed JSON-RPC message.
pub fn write_message<W: Write>(output: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_string(message)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}

/// Server state: open documents and the provider schemas.
pub struct Server {
    documents: HashMap<String, String>,
    schemas: Vec<ProviderSchema>,
    /// Set by `exit`; the serve loop stops after the current message.
    pub exited: bool,
}

impl Server {
    /// A server with no open documents.
    pub fn new(schemas: Vec<ProviderSchema>) -> Self {
        Server {
            documents: HashMap::new(),
            schemas,
            exited: false,
        }
    }

    /// Handle one incoming message and return the messages to send back.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request we never sent: nothing to do.
            return Vec::new();
        };
        match method {
            "initialize" => respond(id, initialize_result()),
            "initialized" => Vec::new(),
            "shutdown" => respond(id, Value::Null),
            "exit" => {
                self.exited = true;
                Vec::new()
            }
            "textDocument/didOpen" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                let text = str_param(&params, &["textDocument", "text"]);
                self.set_document(uri, text)
            }
            "textDocument/didChange" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                // Full-document sync: the last change carries the whole text.
                let text = params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .and_then(|changes| changes.last())
                    .and_then(|c| c.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.set_document(uri, text)
            }
            "textDocument/didClose" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                self.documents.remove(&uri);
                vec![publish(&uri, Vec::new())]
            }
            "textDocument/formatting" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                let edits = self.documents.get(&uri).map(|text| {
                    let formatted = format_source(text);
                    if formatted == *text {
                        Vec::new()
                    } else {
                        vec![json!({
                            "range": { "start": {"line": 0, "character": 0}, "end": end_of(text) },
                            "newText": formatted,
                        })]
                    }
                });
                respond(id, json!(edits.unwrap_or_default()))
            }
            "textDocument/hover" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                let result = self.hover(&uri, &params).unwrap_or(Value::Null);
                respond(id, result)
            }
            "textDocument/definition" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                let result = self.definition(&uri, &params).unwrap_or(Value::Null);
                respond(id, result)
            }
            "textDocument/completion" => {
                let uri = str_param(&params, &["textDocument", "uri"]);
                let items = self.completion(&uri, &params);
                respond(id, json!({ "isIncomplete": false, "items": items }))
            }
            _ => match id {
                Some(id) => vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": METHOD_NOT_FOUND, "message": format!("method not found: {method}") },
                })],
                None => Vec::new(),
            },
        }
    }

    fn set_document(&mut self, uri: String, text: String) -> Vec<Value> {
        let diagnostics = diagnostics_for(&text);
        self.documents.insert(uri.clone(), text);
        vec![publish(&uri, diagnostics)]
    }

    /// The parsed document, if it parses. Hover and definition need the AST.
    fn parsed(&self, uri: &str) -> Option<(&str, Ast)> {
        let text = self.documents.get(uri)?;
        let ast = parse(tokenize(text).ok()?).ok()?;
        Some((text.as_str(), ast))
    }

    fn hover(&self, uri: &str, params: &Value) -> Option<Value> {
        let (text, ast) = self.parsed(uri)?;
        let (line, character) = position(params)?;
        let word = word_at(text, line, character)?;
        let base = word.split('.').next().unwrap_or(&word);

        // A provider resource type: show its attributes.
        if let Some((schema, spec)) = self.resource_type(&word) {
            let mut md = format!("**{}** (provider `{}`)\n\n", spec.type_name, schema.name);
            if let Some(description) = &spec.description {
                md.push_str(&format!("{description}\n\n"));
            }
            for attr in &spec.attributes {
                md.push_str(&format!(
                    "- `{}`: {:?}{}\n",
                    attr.name,
                    attr.value_type,
                    if attr.required { " (required)" } else { "" }
                ));
            }
            return Some(json!({ "contents": { "kind": "markdown", "value": md } }));
        }

        // A declared resource: show its type and id.
        let block = resource_named(&ast, base)?;
        let md = format!(
            "**resource** `{}.{}`",
            block.type_name.as_deref().unwrap_or("?"),
            base
        );
        Some(json!({ "contents": { "kind": "markdown", "value": md } }))
    }

    fn definition(&self, uri: &str, params: &Value) -> Option<Value> {
        let (text, ast) = self.parsed(uri)?;
        let (line, character) = position(params)?;
        let word = word_at(text, line, character)?;
        let base = word.split('.').next().unwrap_or(&word);
        let block = resource_named(&ast, base)?;
        let start = json!({ "line": block.line.saturating_sub(1), "character": block.col.saturating_sub(1) });
        let end = json!({
            "line": block.line.saturating_sub(1),
            "character": block.col.saturating_sub(1) + block.kind.chars().count(),
        });
        Some(json!({ "uri": uri, "range": { "start": start, "end": end } }))
    }

    fn completion(&self, uri: &str, params: &Value) -> Vec<Value> {
        let Some(text) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some((line, character)) = position(params) else {
            return Vec::new();
        };
        let prefix: String = text
            .lines()
            .nth(line)
            .unwrap_or("")
            .chars()
            .take(character)
            .collect();
        let trimmed = prefix.trim_start();

        // After `resource `, offer provider resource types.
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            if !rest.contains(' ') {
                return self
                    .schemas
                    .iter()
                    .flat_map(|s| s.resources.iter())
                    .map(|r| {
                        json!({
                            "label": r.type_name,
                            "kind": KIND_CLASS,
                            "detail": r.description.clone().unwrap_or_default(),
                        })
                    })
                    .collect();
            }
        }

        // Elsewhere, offer the resource names declared in this document.
        let Some((_, ast)) = self.parsed(uri) else {
            return Vec::new();
        };
        ast.blocks
            .iter()
            .filter(|b| b.kind == "resource")
            .filter_map(|b| b.name.as_deref())
            .map(|name| json!({ "label": name, "kind": KIND_VARIABLE }))
            .collect()
    }

    /// The schema and resource spec for a fully qualified type name, if any provider knows it.
    fn resource_type(
        &self,
        name: &str,
    ) -> Option<(
        &ProviderSchema,
        &tpt_bastion_providers::schema::ResourceTypeSpec,
    )> {
        self.schemas.iter().find_map(|schema| {
            schema
                .resources
                .iter()
                .find(|r| r.type_name == name)
                .map(|r| (schema, r))
        })
    }
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": 1,
            "documentFormattingProvider": true,
            "hoverProvider": true,
            "definitionProvider": true,
            "completionProvider": { "triggerCharacters": [" ", "."] },
        },
        "serverInfo": { "name": "bastion-lsp", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn respond(id: Option<Value>, result: Value) -> Vec<Value> {
    match id {
        Some(id) => vec![json!({ "jsonrpc": "2.0", "id": id, "result": result })],
        None => Vec::new(),
    }
}

fn publish(uri: &str, diagnostics: Vec<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": diagnostics },
    })
}

/// The compiler's error for `text`, shown as a diagnostic at its position.
fn diagnostics_for(text: &str) -> Vec<Value> {
    match compile_source(text) {
        Ok(_) => Vec::new(),
        Err(err) => vec![diagnostic(text, &err)],
    }
}

fn diagnostic(text: &str, err: &DslError) -> Value {
    let (start_line, start_char, width) = match err.location() {
        Some((line, col)) => {
            let line0 = line.saturating_sub(1);
            let col0 = col.saturating_sub(1);
            let width = text
                .lines()
                .nth(line0)
                .map(|l| identifier_len(l, col0))
                .unwrap_or(1)
                .max(1);
            (line0, col0, width)
        }
        None => (0, 0, 1),
    };
    json!({
        "range": {
            "start": { "line": start_line, "character": start_char },
            "end": { "line": start_line, "character": start_char + width },
        },
        "severity": SEVERITY_ERROR,
        "source": "bastion",
        "message": err.message(),
    })
}

/// End position of the whole document, for replacing it in a formatting edit.
fn end_of(text: &str) -> Value {
    let lines = text.split('\n').collect::<Vec<_>>();
    let last = lines.last().copied().unwrap_or("");
    json!({
        "line": lines.len().saturating_sub(1),
        "character": last.chars().count(),
    })
}

/// Length of the identifier (letters, digits, `_`, `.`) starting at `col0`.
fn identifier_len(line: &str, col0: usize) -> usize {
    line.chars()
        .skip(col0)
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .count()
}

/// The dotted word under a zero-based `(line, character)` position.
fn word_at(text: &str, line: usize, character: usize) -> Option<String> {
    let text_line: Vec<char> = text.lines().nth(line)?.chars().collect();
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_' || *c == '.';
    if character > text_line.len() {
        return None;
    }
    let mut start = character;
    while start > 0 && is_word(&text_line[start - 1]) {
        start -= 1;
    }
    let mut end = character;
    while end < text_line.len() && is_word(&text_line[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(text_line[start..end].iter().collect())
}

/// The first resource block whose local name is `name`.
fn resource_named<'a>(ast: &'a Ast, name: &str) -> Option<&'a Block> {
    ast.blocks
        .iter()
        .find(|b| b.kind == "resource" && b.name.as_deref() == Some(name))
}

fn position(params: &Value) -> Option<(usize, usize)> {
    let pos = params.get("position")?;
    let line = pos.get("line")?.as_u64()? as usize;
    let character = pos.get("character")?.as_u64()? as usize;
    Some((line, character))
}

fn str_param(params: &Value, path: &[&str]) -> String {
    let mut current = params;
    for key in path {
        match current.get(key) {
            Some(next) => current = next,
            None => return String::new(),
        }
    }
    current.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: u64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    fn open(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
        server.handle(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": uri, "text": text } },
        }))
    }

    fn server() -> Server {
        Server::new(vec![tpt_bastion_provider_aws_schema()])
    }

    /// A small schema for tests, so the tests do not depend on the provider crates.
    fn tpt_bastion_provider_aws_schema() -> ProviderSchema {
        use tpt_bastion_providers::schema::{
            AttributeSpec, ProviderSchema as S, ResourceTypeSpec, ValueType,
        };
        S::builder("aws", "0.1.0")
            .resource(
                ResourceTypeSpec::builder("aws_s3_bucket")
                    .description("An S3 bucket.")
                    .attribute(AttributeSpec::required("bucket", ValueType::String))
                    .build(),
            )
            .build()
    }

    #[test]
    fn publishes_a_diagnostic_at_the_error() {
        let mut s = server();
        let out = open(
            &mut s,
            "file:///a.bcl",
            "resource aws_s3_bucket b {\n  bucket = nope.bucket\n}\n",
        );
        let diags = &out[0]["params"]["diagnostics"];
        assert_eq!(diags.as_array().unwrap().len(), 1);
        assert_eq!(diags[0]["range"]["start"]["line"], 1);
        assert_eq!(diags[0]["range"]["start"]["character"], 11);
        assert!(diags[0]["message"]
            .as_str()
            .unwrap()
            .contains("unknown `nope`"));
    }

    #[test]
    fn clean_document_has_no_diagnostics() {
        let mut s = server();
        let out = open(
            &mut s,
            "file:///a.bcl",
            "resource aws_s3_bucket b {\n bucket = \"x\"\n}\n",
        );
        assert!(out[0]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn formatting_replaces_the_whole_document() {
        let mut s = server();
        open(
            &mut s,
            "file:///a.bcl",
            "resource aws_s3_bucket b {\nbucket = \"x\"\n}\n",
        );
        let out = s.handle(&request(
            2,
            "textDocument/formatting",
            json!({ "textDocument": { "uri": "file:///a.bcl" }, "options": {} }),
        ));
        let edits = out[0]["result"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0]["newText"],
            "resource aws_s3_bucket b {\n    bucket = \"x\"\n}\n"
        );
    }

    #[test]
    fn go_to_definition_jumps_to_the_resource_block() {
        let mut s = server();
        open(
            &mut s,
            "file:///a.bcl",
            "resource aws_s3_bucket a { bucket = \"x\" }\nresource aws_s3_bucket b { bucket = a.bucket }\n",
        );
        // Cursor on `a` in `a.bucket` (line 1, character 40).
        let out = s.handle(&request(
            3,
            "textDocument/definition",
            json!({ "textDocument": { "uri": "file:///a.bcl" }, "position": { "line": 1, "character": 42 } }),
        ));
        assert_eq!(out[0]["result"]["range"]["start"]["line"], 0);
        assert_eq!(out[0]["result"]["range"]["start"]["character"], 0);
    }

    #[test]
    fn hover_shows_the_provider_schema_for_a_resource_type() {
        let mut s = server();
        open(
            &mut s,
            "file:///a.bcl",
            "resource aws_s3_bucket b { bucket = \"x\" }\n",
        );
        let out = s.handle(&request(
            4,
            "textDocument/hover",
            json!({ "textDocument": { "uri": "file:///a.bcl" }, "position": { "line": 0, "character": 12 } }),
        ));
        let value = out[0]["result"]["contents"]["value"].as_str().unwrap();
        assert!(value.contains("aws_s3_bucket"));
        assert!(value.contains("`bucket`"));
    }

    #[test]
    fn completes_resource_types_after_the_keyword() {
        let mut s = server();
        open(&mut s, "file:///a.bcl", "resource ");
        let out = s.handle(&request(
            5,
            "textDocument/completion",
            json!({ "textDocument": { "uri": "file:///a.bcl" }, "position": { "line": 0, "character": 9 } }),
        ));
        let labels: Vec<&str> = out[0]["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, vec!["aws_s3_bucket"]);
    }

    #[test]
    fn unknown_requests_get_an_error_and_notifications_get_nothing() {
        let mut s = server();
        let out = s.handle(&request(9, "textDocument/nope", Value::Null));
        assert_eq!(out[0]["error"]["code"], METHOD_NOT_FOUND);
        let none =
            s.handle(&json!({ "jsonrpc": "2.0", "method": "$/cancelRequest", "params": {} }));
        assert!(none.is_empty());
    }

    #[test]
    fn framed_messages_round_trip_through_serve() {
        let input = {
            let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} })
                .to_string();
            let exit = json!({ "jsonrpc": "2.0", "method": "exit" }).to_string();
            format!(
                "Content-Length: {}\r\n\r\n{body}Content-Length: {}\r\n\r\n{exit}",
                body.len(),
                exit.len()
            )
        };
        let mut reader = io::Cursor::new(input.into_bytes());
        let mut output = Vec::new();
        let mut s = server();
        serve(&mut reader, &mut output, &mut s).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("Content-Length: "));
        assert!(text.contains("\"textDocumentSync\":1"));
        assert!(s.exited);
    }
}
