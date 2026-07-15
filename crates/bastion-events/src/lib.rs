// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # bastion-events
//!
//! Event-driven reconciliation for [TPT Bastion][bastion] (Phase 7 of the
//! roadmap).
//!
//! Infrastructure often needs to react to things that happen *outside* a
//! `plan`/`apply` cycle: a config file is edited, a Git push lands, a webhook
//! fires from a CI system or a cloud control plane. This crate provides a
//! small, pluggable model for that:
//!
//! - [`InfraEvent`] — the events the system understands.
//! - [`EventSource`] — the pluggable interface an event backend implements.
//!   Ships with [`FileEventSource`] (watch files for changes) and
//!   [`WebhookEventSource`] (accept HTTP POSTs). A future [TPT Flux] backend
//!   would implement the same trait.
//! - [`run_forever`] / [`run_with_limit`] — the reconcile-on-event loop that
//!   calls your handler for every event, so infrastructure is brought back in
//!   line as the world changes.
//!
//! [bastion]: https://bastion.tpt.dev
//! [TPT Flux]: https://bastion.tpt.dev/flux

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

/// An external event that may require infrastructure reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfraEvent {
    /// A watched file's modification time advanced.
    FileChanged(PathBuf),
    /// An inbound webhook request.
    Webhook {
        /// HTTP method, e.g. `POST`.
        method: String,
        /// Request path, e.g. `/hook`.
        path: String,
        /// Raw request body.
        body: String,
    },
    /// A periodic tick (used by polling sources).
    Tick,
}

impl std::fmt::Display for InfraEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InfraEvent::FileChanged(p) => write!(f, "file changed: {}", p.display()),
            InfraEvent::Webhook { method, path, .. } => {
                write!(f, "webhook: {} {}", method, path)
            }
            InfraEvent::Tick => write!(f, "tick"),
        }
    }
}

/// A pluggable source of [`InfraEvent`]s.
///
/// Implement this to back Bastion with any event system. The built-in
/// [`FileEventSource`] and [`WebhookEventSource`] cover the common cases, and a
/// future [TPT Flux] integration would implement the same trait.
///
/// [TPT Flux]: https://bastion.tpt.dev/flux
pub trait EventSource: Send {
    /// Block until the next event is available, or return `None` if the source
    /// is exhausted and should no longer be polled.
    fn next_event(&mut self) -> Option<InfraEvent>;
}

/// Watches a set of files and emits [`InfraEvent::FileChanged`] when any of
/// their modification times advance.
pub struct FileEventSource {
    paths: Vec<PathBuf>,
    seen: HashMap<PathBuf, SystemTime>,
    poll: Duration,
}

impl FileEventSource {
    /// Create a watcher over `paths`, polling every `poll` interval.
    pub fn new(paths: Vec<PathBuf>, poll: Duration) -> Self {
        Self {
            paths,
            seen: HashMap::new(),
            poll,
        }
    }

    fn mtime(path: &std::path::Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
    }
}

impl EventSource for FileEventSource {
    fn next_event(&mut self) -> Option<InfraEvent> {
        loop {
            for path in &self.paths {
                let mtime = Self::mtime(path);
                match (self.seen.get(path), mtime) {
                    (None, Some(t)) => {
                        self.seen.insert(path.clone(), t);
                    }
                    (Some(old), Some(t)) if t > *old => {
                        self.seen.insert(path.clone(), t);
                        return Some(InfraEvent::FileChanged(path.clone()));
                    }
                    _ => {}
                }
            }
            thread::sleep(self.poll);
        }
    }
}

/// Accepts inbound HTTP `POST` requests and emits [`InfraEvent::Webhook`].
///
/// A minimal, dependency-free HTTP listener suitable for control-plane
/// webhooks; pair it with a reverse proxy for TLS/auth in production.
pub struct WebhookEventSource {
    rx: Receiver<InfraEvent>,
    local_addr: std::net::SocketAddr,
    _handle: JoinHandle<()>,
}

impl WebhookEventSource {
    /// Bind the webhook listener to `addr` (e.g. `127.0.0.1:8080`) and start
    /// accepting connections on a background thread.
    pub fn bind(addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let local_addr = listener.local_addr()?;
        let (tx, rx) = channel::<InfraEvent>();
        let handle = thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Some(event) = read_webhook(stream) {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self {
            rx,
            local_addr,
            _handle: handle,
        })
    }

    /// The address the listener is bound to (useful with `127.0.0.1:0`).
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }
}

impl EventSource for WebhookEventSource {
    fn next_event(&mut self) -> Option<InfraEvent> {
        self.rx.recv().ok()
    }
}

/// Read a single HTTP request from `stream` and turn a `POST` into an event.
fn read_webhook(mut stream: TcpStream) -> Option<InfraEvent> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    // Read headers up to the blank line separating them from the body.
    while buf.len() < 1 << 16 {
        if stream.read(&mut byte).ok()? == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header_text = String::from_utf8_lossy(&buf);
    let mut lines = header_text.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }

    let header_len = buf.len();
    let mut body = vec![0u8; content_length];
    let mut filled = 0;
    while filled < content_length {
        match stream.read(&mut body[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }
    let _ = header_len;

    let _ = stream.shutdown(Shutdown::Both);

    if method.eq_ignore_ascii_case("POST") {
        Some(InfraEvent::Webhook {
            method,
            path,
            body: String::from_utf8_lossy(&body[..filled]).into_owned(),
        })
    } else {
        // Acknowledge other methods without producing an event.
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        None
    }
}

/// Run the reconcile-on-event loop forever, calling `handler` for every event.
///
/// This returns `!` — it only stops when the process is terminated.
pub fn run_forever<S, F>(source: &mut S, mut handler: F) -> !
where
    S: EventSource,
    F: FnMut(&InfraEvent),
{
    loop {
        if let Some(event) = source.next_event() {
            handler(&event);
        }
    }
}

/// Run the reconcile-on-event loop until `max_events` have been handled.
///
/// Returns the number of events handled. Useful for tests and one-shot runs.
pub fn run_with_limit<S, F>(source: &mut S, max_events: usize, mut handler: F) -> usize
where
    S: EventSource,
    F: FnMut(&InfraEvent),
{
    let mut handled = 0;
    while handled < max_events {
        match source.next_event() {
            Some(event) => {
                handler(&event);
                handled += 1;
            }
            None => break,
        }
    }
    handled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_source_detects_change() {
        let dir = std::env::temp_dir().join(format!("bastion-evt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("watch.txt");
        std::fs::write(&path, b"v1").unwrap();

        let mut source = FileEventSource::new(vec![path.clone()], Duration::from_millis(10));
        // Mutate the file from another thread after a short delay.
        let writer = path.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(60));
            std::fs::write(&writer, b"v2").unwrap();
        });

        let event = source.next_event();
        handle.join().unwrap();
        assert_eq!(event, Some(InfraEvent::FileChanged(path.clone())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn webhook_source_receives_event() {
        let source = WebhookEventSource::bind("127.0.0.1:0").unwrap();
        let addr = source.local_addr();

        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            let payload = "hello";
            let req = format!(
                "POST /hook HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
                payload.len(),
                payload
            );
            stream.write_all(req.as_bytes()).unwrap();
        });

        let mut source = source;
        let event = source.next_event();
        client.join().unwrap();
        match event {
            Some(InfraEvent::Webhook { method, path, body }) => {
                assert_eq!(method, "POST");
                assert_eq!(path, "/hook");
                assert_eq!(body, "hello");
            }
            other => panic!("expected webhook event, got {other:?}"),
        }
    }
}
