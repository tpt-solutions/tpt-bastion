// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A minimal, dependency-free HTTP server for the dashboard.
//!
//! Binds `127.0.0.1` only. Serves the embedded SPA at `/` and a small JSON API
//! under `/api/*`. When a token is configured, every API request must present
//! it (`Authorization: Bearer <token>` or `X-Bastion-Token`). Responses carry
//! hardening headers (CSP, nosniff, no-store).

use crate::api;
use crate::assets::INDEX_HTML;
use crate::UiConfig;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

/// Security headers applied to every response.
const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("Content-Security-Policy", "default-src 'self'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src 'none'; connect-src 'self'"),
    ("X-Content-Type-Options", "nosniff"),
    ("Cache-Control", "no-store"),
];

/// Start the dashboard and block, serving until the process exits.
pub fn run(config: UiConfig) -> anyhow::Result<()> {
    let addr = format!("127.0.0.1:{}", config.port);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| anyhow::anyhow!("could not bind dashboard to {addr}: {e}"))?;
    println!("Bastion dashboard running at http://{addr}");
    if config.token.is_some() {
        println!("  (API requires the configured bearer token)");
    }
    if config.allow_apply {
        println!("  (apply is ENABLED — POST /api/apply will mutate infrastructure)");
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let cfg = config.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, &cfg);
        });
    }
    Ok(())
}

fn handle(mut stream: std::net::TcpStream, config: &UiConfig) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 1 << 16 {
        if stream.read(&mut byte)? == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    let request_line = match lines.next() {
        Some(l) => l,
        None => return Ok(()),
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/").to_string();

    // Collect headers. Names are case-insensitive; values (tokens, origins)
    // keep their original case.
    let mut presented: Option<String> = None;
    let mut host: Option<String> = None;
    let mut origin: Option<String> = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("x-bastion-token") {
            presented = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("authorization") {
            let is_bearer = value
                .get(..7)
                .is_some_and(|s| s.eq_ignore_ascii_case("bearer "));
            if is_bearer {
                presented = Some(value[7..].trim().to_string());
            }
        } else if name.eq_ignore_ascii_case("host") {
            host = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.to_string());
        }
    }

    let is_api = path.starts_with("/api/");
    if is_api {
        if let Some(expected) = &config.token {
            let ok = match &presented {
                Some(p) => constant_time_eq(p.as_bytes(), expected.as_bytes()),
                None => false,
            };
            if !ok {
                return respond(
                    &mut stream,
                    401,
                    "application/json",
                    b"{\"error\":\"unauthorized\"}",
                );
            }
        }
    }

    // CSRF guard for the mutating endpoint: a browser on another site cannot
    // set a matching Host, and sends a foreign Origin. Reject both.
    if method == "POST" && !origin_is_local(host.as_deref(), origin.as_deref(), config.port) {
        return respond(
            &mut stream,
            403,
            "application/json",
            b"{\"error\":\"cross-origin request rejected\"}",
        );
    }

    match (method, path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        ("GET", "/api/info") => json(stream, Ok(api::info(config))),
        ("GET", "/api/compile") => json(stream, api::compile(config)),
        ("GET", "/api/plan") => json(stream, api::plan(config)),
        ("GET", "/api/state") => json(stream, api::state(config)),
        ("GET", "/api/history") => json(stream, api::history(config)),
        ("GET", "/api/drift") => json(stream, api::drift(config)),
        ("GET", "/api/providers") => json(stream, Ok(api::providers())),
        ("POST", "/api/apply") => {
            if !config.allow_apply {
                return respond(
                    &mut stream,
                    403,
                    "application/json",
                    b"{\"error\":\"apply not enabled\"}",
                );
            }
            match api::apply(config) {
                Ok(out) => {
                    let body = format!(
                        "{{\"ok\":true,\"output\":{}}}",
                        serde_json::to_string(&out).unwrap_or_default()
                    );
                    respond(&mut stream, 200, "application/json", body.as_bytes())
                }
                Err(e) => respond(
                    &mut stream,
                    500,
                    "application/json",
                    format!(
                        "{{\"error\":{}}}",
                        serde_json::to_string(&e).unwrap_or_default()
                    )
                    .as_bytes(),
                ),
            }
        }
        _ => respond(
            &mut stream,
            404,
            "application/json",
            b"{\"error\":\"not found\"}",
        ),
    }
}

fn json(
    stream: std::net::TcpStream,
    result: Result<serde_json::Value, String>,
) -> std::io::Result<()> {
    match result {
        Ok(v) => {
            let body = serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into());
            respond(&mut { stream }, 200, "application/json", body.as_bytes())
        }
        Err(e) => respond(
            &mut { stream },
            500,
            "application/json",
            format!(
                "{{\"error\":{}}}",
                serde_json::to_string(&e).unwrap_or_default()
            )
            .as_bytes(),
        ),
    }
}

fn respond(
    stream: &mut std::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in SECURITY_HEADERS {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Whether a request's Host and (optional) Origin point at this dashboard.
///
/// Host must be the loopback address on our port. If an Origin header is
/// present it must be the same loopback origin; a missing Origin (non-browser
/// clients such as curl) is allowed.
fn origin_is_local(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let local_hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let Some(host) = host else { return false };
    if !local_hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return false;
    }
    match origin {
        None => true,
        Some(o) => local_hosts
            .iter()
            .any(|h| o.eq_ignore_ascii_case(&format!("http://{h}"))),
    }
}

/// Constant-time comparison to avoid leaking the token via timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
