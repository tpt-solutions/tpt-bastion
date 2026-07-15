// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The host↔plugin ABI and the isolated subprocess runtime.
//!
//! The ABI is a line-delimited JSON protocol ([`AbiRequest`] / [`AbiResponse`])
//! over a bidirectional byte stream (typically a child process's stdin/stdout).
//! It is deliberately transport-agnostic: the same messages drive an in-process
//! [`crate::plugin::NativeRuntime`] today and a Wasm host tomorrow, because the
//! Wasm module would simply call the same `create`/`delete`/`schema` entry points.
//!
//! [`SubprocessRuntime`] is the production isolation boundary: each provider runs
//! as its own OS process, so a crashing or misbehaving provider cannot corrupt
//! the host. The host talks to it purely through the ABI.

use crate::error::RuntimeError;
use crate::plugin::{DataSourceRequest, ProviderPlugin, ProviderRuntime, ResourceRequest};
use crate::schema::ProviderSchema;
use bastion_engine::provider::AppliedResource;
use bastion_ir::Value;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

/// A request the host sends to a plugin over the ABI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "p")]
pub enum AbiRequest {
    /// Return the provider schema.
    Schema,
    /// Create a resource.
    Create(ResourceRequest),
    /// Read a resource.
    Read(ResourceRequest),
    /// Update a resource.
    Update(ResourceRequest),
    /// Delete a resource.
    Delete(ResourceRequest),
    /// Query a data source.
    DataSource(DataSourceRequest),
}

/// A response a plugin sends back to the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "p")]
pub enum AbiResponse {
    /// A provider schema.
    Schema(ProviderSchema),
    /// A created/updated resource.
    Applied(AppliedResource),
    /// A read result (or absence).
    Read(Option<AppliedResource>),
    /// Acknowledgement of a delete.
    Deleted,
    /// A data source result.
    Data(Value),
    /// A failure.
    Error { message: String },
}

/// Drives a plugin over a byte stream until EOF on the input.
///
/// Each input line is one [`AbiRequest`]; the corresponding [`AbiResponse`]
/// (one per request, in order) is written as a line to the output. A provider
/// binary's `main` typically calls `PluginServer::serve(&my_plugin, stdin(),
/// stdout())` and exits when the host closes the pipe.
pub struct PluginServer;

impl PluginServer {
    /// Serve `plugin` from `input` to `output` until `input` reaches EOF.
    pub fn serve<P: ProviderPlugin>(
        plugin: &P,
        input: impl Read,
        mut output: impl Write,
    ) -> Result<(), String> {
        let mut reader = BufReader::new(input);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF: host disconnected.
                Ok(_) => {}
                Err(e) => return Err(format!("abi read error: {e}")),
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let req: AbiRequest =
                serde_json::from_str(line).map_err(|e| format!("abi decode error: {e}"))?;
            let resp = handle(plugin, req);
            let text =
                serde_json::to_string(&resp).map_err(|e| format!("abi encode error: {e}"))?;
            writeln!(output, "{text}").map_err(|e| format!("abi write error: {e}"))?;
            let _ = output.flush();
        }
        Ok(())
    }
}

/// Translate one ABI request into a response for `plugin`.
fn handle<P: ProviderPlugin>(plugin: &P, req: AbiRequest) -> AbiResponse {
    match req {
        AbiRequest::Schema => AbiResponse::Schema(plugin.schema()),
        AbiRequest::Create(r) => match plugin.create(r) {
            Ok(a) => AbiResponse::Applied(a),
            Err(e) => AbiResponse::Error {
                message: e.to_string(),
            },
        },
        AbiRequest::Read(r) => match plugin.read(r) {
            Ok(a) => AbiResponse::Read(a),
            Err(e) => AbiResponse::Error {
                message: e.to_string(),
            },
        },
        AbiRequest::Update(r) => match plugin.update(r) {
            Ok(a) => AbiResponse::Applied(a),
            Err(e) => AbiResponse::Error {
                message: e.to_string(),
            },
        },
        AbiRequest::Delete(r) => match plugin.delete(r) {
            Ok(()) => AbiResponse::Deleted,
            Err(e) => AbiResponse::Error {
                message: e.to_string(),
            },
        },
        AbiRequest::DataSource(r) => match plugin.data_source(r) {
            Ok(v) => AbiResponse::Data(v),
            Err(e) => AbiResponse::Error {
                message: e.to_string(),
            },
        },
    }
}

/// A [`ProviderRuntime`] that runs each provider as a separate OS process.
///
/// Isolation is real: a provider that panics, leaks, or is compromised cannot
/// touch the host's memory or other providers. Communication is exclusively the
/// ABI, so the host's API surface is exactly what the protocol allows.
pub struct SubprocessRuntime {
    /// Path to the provider server binary to spawn.
    binary: PathBuf,
}

impl SubprocessRuntime {
    /// A runtime that spawns `binary` to host plugins.
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        SubprocessRuntime {
            binary: binary.into(),
        }
    }
}

impl ProviderRuntime for SubprocessRuntime {
    fn load(&self, name: &str) -> Result<Arc<dyn ProviderPlugin>, RuntimeError> {
        let mut child = Command::new(&self.binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RuntimeError::Process {
                name: name.to_string(),
                message: e.to_string(),
            })?;

        let conn = SubprocessConn {
            stdin: child.stdin.take().ok_or_else(|| RuntimeError::Process {
                name: name.to_string(),
                message: "no stdin handle".into(),
            })?,
            stdout: BufReader::new(child.stdout.take().ok_or_else(|| RuntimeError::Process {
                name: name.to_string(),
                message: "no stdout handle".into(),
            })?),
            child,
        };

        let mut plugin = SubprocessPlugin {
            conn: Mutex::new(conn),
            schema: Mutex::new(None),
            name: name.to_string(),
            version: String::new(),
        };

        // Eagerly fetch the schema so `name()`/`version()`/`schema()` can return
        // without a round-trip each call, and to fail fast if the process is dead.
        let schema = plugin
            .request(AbiRequest::Schema)
            .map_err(|m| RuntimeError::Process {
                name: name.to_string(),
                message: m,
            })?;
        if let AbiResponse::Schema(s) = schema {
            *plugin.schema.lock().unwrap() = Some(s.clone());
            plugin.version = s.version.clone();
        }

        Ok(Arc::new(plugin))
    }

    fn names(&self) -> Vec<String> {
        // The subprocess runtime hosts whatever binary it is pointed at; callers
        // discover concrete names by loading and reading the schema.
        Vec::new()
    }
}

struct SubprocessConn {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    child: Child,
}

/// A [`ProviderPlugin`] backed by a child process speaking the ABI.
struct SubprocessPlugin {
    conn: Mutex<SubprocessConn>,
    schema: Mutex<Option<ProviderSchema>>,
    name: String,
    version: String,
}

impl SubprocessPlugin {
    /// Send one request and read its single response, serializing access to the
    /// shared connection so parallel `create`s from the executor stay consistent.
    fn request(&self, req: AbiRequest) -> Result<AbiResponse, String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "plugin connection poisoned".to_string())?;
        let text = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        writeln!(conn.stdin, "{text}").map_err(|e| e.to_string())?;
        let _ = conn.stdin.flush();
        let mut line = String::new();
        conn.stdout
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(line.trim()).map_err(|e| e.to_string())
    }
}

impl Drop for SubprocessPlugin {
    fn drop(&mut self) {
        // Closing stdin signals the child to exit its serve loop; reap it.
        if let Ok(mut conn) = self.conn.lock() {
            let _ = conn.stdin.flush();
            let _ = conn.child.kill();
            let _ = conn.child.wait();
        }
    }
}

impl ProviderPlugin for SubprocessPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn schema(&self) -> ProviderSchema {
        self.schema
            .lock()
            .ok()
            .and_then(|s| s.clone())
            .unwrap_or_else(|| ProviderSchema {
                name: self.name.clone(),
                version: String::new(),
                resources: Vec::new(),
                data_sources: Vec::new(),
            })
    }

    fn create(&self, req: ResourceRequest) -> Result<AppliedResource, crate::error::ProviderError> {
        match self.request(AbiRequest::Create(req)) {
            Ok(AbiResponse::Applied(a)) => Ok(a),
            Ok(AbiResponse::Error { message }) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message,
            }),
            Ok(_) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: "unexpected ABI response".into(),
            }),
            Err(m) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: m,
            }),
        }
    }

    fn read(
        &self,
        req: ResourceRequest,
    ) -> Result<Option<AppliedResource>, crate::error::ProviderError> {
        match self.request(AbiRequest::Read(req)) {
            Ok(AbiResponse::Read(a)) => Ok(a),
            Ok(AbiResponse::Error { message }) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message,
            }),
            Ok(_) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: "unexpected ABI response".into(),
            }),
            Err(m) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: m,
            }),
        }
    }

    fn update(&self, req: ResourceRequest) -> Result<AppliedResource, crate::error::ProviderError> {
        match self.request(AbiRequest::Update(req)) {
            Ok(AbiResponse::Applied(a)) => Ok(a),
            Ok(AbiResponse::Error { message }) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message,
            }),
            Ok(_) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: "unexpected ABI response".into(),
            }),
            Err(m) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: m,
            }),
        }
    }

    fn delete(&self, req: ResourceRequest) -> Result<(), crate::error::ProviderError> {
        match self.request(AbiRequest::Delete(req)) {
            Ok(AbiResponse::Deleted) => Ok(()),
            Ok(AbiResponse::Error { message }) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message,
            }),
            Ok(_) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: "unexpected ABI response".into(),
            }),
            Err(m) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: m,
            }),
        }
    }

    fn data_source(&self, req: DataSourceRequest) -> Result<Value, crate::error::ProviderError> {
        match self.request(AbiRequest::DataSource(req)) {
            Ok(AbiResponse::Data(v)) => Ok(v),
            Ok(AbiResponse::Error { message }) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message,
            }),
            Ok(_) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: "unexpected ABI response".into(),
            }),
            Err(m) => Err(crate::error::ProviderError::Internal {
                provider: self.name.clone(),
                message: m,
            }),
        }
    }
}
