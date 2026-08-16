//! A loopback-only HTTP/JSON bridge for the browser extension
//! (`extension/`), which can't dial the named-pipe transport a native
//! client uses — browsers have no named-pipe API. This is the local API
//! Gateway's second transport, additive to (not a replacement for) the
//! gRPC-over-named-pipe path every other client uses; it forwards straight
//! into the same `Engine`, so there's still exactly one writer
//! (`docs/ARCHITECTURE.md` §1) regardless of which transport a request
//! arrived on.
//!
//! Bound to `127.0.0.1` only — never `0.0.0.0` — so this never becomes a
//! LAN-reachable endpoint. `tiny_http` (blocking, no async runtime needed)
//! runs on its own OS thread rather than inside the tokio runtime `serve()`
//! already owns.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server};

use vision_core::Engine;

pub const PORT: u16 = 47823;

#[derive(Deserialize)]
struct IngestBrowserRequest {
    url: String,
    text: String,
}

#[derive(Serialize)]
struct IngestBrowserResponse {
    accepted: bool,
    document_id: Option<String>,
    chunks_indexed: Option<usize>,
    error: Option<String>,
}

fn cors_header() -> Header {
    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
}

fn json_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()
}

/// Spawns the bridge on a dedicated thread; returns immediately. Panics in
/// the spawned thread (e.g. the port is already in use — a second daemon
/// instance can't reach this since `single_instance` already refuses to
/// start a second process) surface as the thread simply dying, which is
/// acceptable here: the named-pipe transport is still the primary path and
/// keeps working regardless.
pub fn spawn(engine: Arc<Engine>) {
    std::thread::spawn(move || {
        let server = match Server::http(("127.0.0.1", PORT)) {
            Ok(server) => server,
            Err(e) => {
                eprintln!("vision-daemon: http bridge failed to bind 127.0.0.1:{PORT}: {e}");
                return;
            }
        };
        eprintln!("vision-daemon: browser extension bridge listening on http://127.0.0.1:{PORT}");

        for request in server.incoming_requests() {
            let engine = engine.clone();
            handle(request, &engine);
        }
    });
}

fn handle(mut request: tiny_http::Request, engine: &Arc<Engine>) {
    // Preflight: extension requests from a background service worker don't
    // trigger CORS preflight in practice (host_permissions bypasses it),
    // but handling OPTIONS costs nothing and covers the popup's page-context
    // fetches too.
    if request.method() == &Method::Options {
        let _ = request.respond(Response::empty(204).with_header(cors_header()));
        return;
    }

    match (request.method(), request.url()) {
        (Method::Get, "/health") => {
            let body = serde_json::json!({ "ok": true }).to_string();
            let _ = request.respond(
                Response::from_string(body)
                    .with_header(json_header())
                    .with_header(cors_header()),
            );
        }
        (Method::Post, "/ingest") => {
            let mut body = String::new();
            if std::io::Read::read_to_string(request.as_reader(), &mut body).is_err() {
                let _ = request.respond(Response::empty(400).with_header(cors_header()));
                return;
            }
            let parsed: Result<IngestBrowserRequest, _> = serde_json::from_str(&body);
            let payload = match parsed {
                Ok(p) => p,
                Err(e) => {
                    let resp = IngestBrowserResponse {
                        accepted: false,
                        document_id: None,
                        chunks_indexed: None,
                        error: Some(format!("bad request: {e}")),
                    };
                    let _ = request.respond(
                        Response::from_string(serde_json::to_string(&resp).unwrap())
                            .with_status_code(400)
                            .with_header(json_header())
                            .with_header(cors_header()),
                    );
                    return;
                }
            };

            let source = vision_proto::IngestSource::Browser as i32;
            let result = vision_core::ingest::run_browser(engine, &payload.url, source, payload.text);
            let resp = match result {
                Ok(outcome) => IngestBrowserResponse {
                    accepted: true,
                    document_id: Some(outcome.document_id),
                    chunks_indexed: Some(outcome.chunks_indexed),
                    error: None,
                },
                Err(e) => IngestBrowserResponse {
                    accepted: false,
                    document_id: None,
                    chunks_indexed: None,
                    error: Some(e.to_string()),
                },
            };
            let _ = request.respond(
                Response::from_string(serde_json::to_string(&resp).unwrap())
                    .with_header(json_header())
                    .with_header(cors_header()),
            );
        }
        _ => {
            let _ = request.respond(Response::empty(404).with_header(cors_header()));
        }
    }
}
