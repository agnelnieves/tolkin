//! End-to-end tests for `tolkin mcp --probe`. These tests cover both
//! transports and the CI refusal posture:
//!
//! - HTTP: an in-process std::net::TcpListener answers minimal HTTP and
//!   serves the MCP initialize + tools/list handshake.
//! - stdio: a small python3 fixture writes JSON-RPC frames over stdout.
//!   When python3 is not on PATH the test is skipped (the probe itself is
//!   already covered by the HTTP path).
//! - CI=1: --probe must refuse with a clear message before any side effect.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-mcp-probe-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run `tolkin mcp ...` with isolated state and an explicit cwd so the
/// project cache lands in a temp dir we control. CI is cleared by default.
fn run_mcp(
    cwd: &Path,
    data_dir: &Path,
    home_dir: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (String, String, bool) {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir)
        .env("HOME", home_dir)
        .env("USERPROFILE", home_dir)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--yes")
        .arg("mcp");
    for (k, v) in extra_env {
        c.env(*k, *v);
    }
    for arg in args {
        c.arg(arg);
    }
    let out = c.output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

// ---------------------------------------------------------------------------
// Minimal HTTP MCP fixture
// ---------------------------------------------------------------------------

/// Spawn a TCP listener that handles JSON-RPC over HTTP. Returns the bound
/// URL (http://127.0.0.1:PORT/mcp) and a join handle for the server thread.
/// The server stops accepting after `max_requests` to keep the test bounded.
fn spawn_http_mcp(max_requests: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let handle = thread::spawn(move || {
        let mut handled = 0;
        for stream in listener.incoming() {
            if handled >= max_requests {
                break;
            }
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            let (req_body, _headers) = match read_http_request(&mut stream) {
                Some(r) => r,
                None => continue,
            };
            let body_json: serde_json::Value =
                serde_json::from_str(&req_body).unwrap_or(serde_json::Value::Null);
            let method = body_json
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let id = body_json
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let response: Option<serde_json::Value> = match method {
                "initialize" => Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "fixture", "version": "0.0.1" }
                    }
                })),
                "tools/list" => Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "create_token",
                                "description": "Create a token. Returns JSON. Example: sk-test-1234567890abcdef0123",
                                "inputSchema": { "type": "object", "properties": { "label": { "type": "string" } } }
                            },
                            {
                                "name": "list_items",
                                "description": "List items. Returns an array of item ids as JSON.",
                                "inputSchema": { "type": "object" }
                            },
                            {
                                "name": "delete_item",
                                "description": "Delete an item by id. Returns the deleted id.",
                                "inputSchema": { "type": "object", "properties": { "id": { "type": "string" } } }
                            }
                        ]
                    }
                })),
                "notifications/initialized" => None,
                _ => Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("unknown method: {method}") }
                })),
            };

            let body = match response {
                Some(v) => serde_json::to_string(&v).unwrap(),
                None => String::new(),
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            handled += 1;
        }
    });
    (url, handle)
}

/// Read a single HTTP request from `stream`. Returns (body, header lines).
/// Bounded by the listener's read timeout; on failure returns None.
fn read_http_request(stream: &mut std::net::TcpStream) -> Option<(String, Vec<String>)> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut headers: Vec<String> = Vec::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
        headers.push(trimmed);
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some((String::from_utf8_lossy(&body).into_owned(), headers))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn http_probe_captures_manifest_redacts_and_cache_hits_on_rerun() {
    let cwd = tmp("http-cwd");
    let data = tmp("http-data");
    let home = tmp("http-home");

    // 4 requests max: initialize + initialized (no response) + tools/list,
    // plus the same on a second probe attempt if anything retries. Tight on
    // purpose so the listener thread terminates cleanly.
    let (url, server) = spawn_http_mcp(8);

    // Config with one HTTP server pointing at our fixture.
    let config_path = cwd.join("mcp.json");
    fs::write(
        &config_path,
        format!(r#"{{ "mcpServers": {{ "tracker": {{ "url": "{url}" }} }} }}"#),
    )
    .unwrap();

    // First run: probe + analyze.
    let (stdout, stderr, ok) = run_mcp(
        &cwd,
        &data,
        &home,
        &[
            config_path.to_str().unwrap(),
            "--probe",
            "tracker",
            "--probe-timeout",
            "5",
        ],
        &[],
    );
    assert!(
        ok,
        "probe run failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );
    assert!(stdout.contains("Per-tool breakdown"), "{stdout}");
    assert!(stdout.contains("create_token"), "{stdout}");
    assert!(stdout.contains("list_items"), "{stdout}");

    // Cache file exists with the pinned shape and the planted secret
    // redacted from the description.
    let cache_path = cwd.join(".tolkin/mcp-manifests/tracker.json");
    assert!(
        cache_path.exists(),
        "cache file missing: {}",
        cache_path.display()
    );
    let cache_text = fs::read_to_string(&cache_path).unwrap();
    let cache_json: serde_json::Value = serde_json::from_str(&cache_text).unwrap();
    assert_eq!(cache_json["v"].as_u64(), Some(1));
    assert_eq!(cache_json["server"].as_str(), Some("tracker"));
    assert_eq!(cache_json["transport"].as_str(), Some("http"));
    assert_eq!(cache_json["source"].as_str(), Some("probe"));
    assert_eq!(cache_json["tools"].as_array().map(|a| a.len()), Some(3));
    assert!(
        !cache_text.contains("sk-test-1234567890abcdef"),
        "redaction must scrub the planted key in the cached file:\n{cache_text}"
    );
    assert!(cache_text.contains("REDACTED"), "{cache_text}");

    // Second run without --probe: the cache supplies the inventory and the
    // basis line names the cache, not the catalog.
    let (stdout2, _stderr2, ok2) = run_mcp(
        &cwd,
        &data,
        &home,
        &[config_path.to_str().unwrap(), "--json"],
        &[],
    );
    assert!(ok2, "cache-hit run failed: {stdout2}");
    let analysis: serde_json::Value = serde_json::from_str(&stdout2).unwrap();
    let basis = analysis["servers"][0]["basis"].as_str().unwrap_or("");
    assert!(
        basis.contains("measured (probed manifest, captured "),
        "basis must name the cache, got: {basis}"
    );
    assert_eq!(analysis["totals"]["measured"].as_u64(), Some(1));

    // Best-effort: drop the listener thread.
    drop(server);
}

#[test]
fn ci_truthy_refuses_probe_before_any_side_effect() {
    let cwd = tmp("ci-cwd");
    let data = tmp("ci-data");
    let home = tmp("ci-home");

    let config_path = cwd.join("mcp.json");
    fs::write(
        &config_path,
        r#"{ "mcpServers": { "tracker": { "command": "node", "args": ["./srv.js"] } } }"#,
    )
    .unwrap();

    let (stdout, stderr, ok) = run_mcp(
        &cwd,
        &data,
        &home,
        &[config_path.to_str().unwrap(), "--probe", "tracker"],
        &[("CI", "1")],
    );
    assert!(
        !ok,
        "CI=1 with --probe must fail:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("refused under CI"), "{combined}");
    assert!(combined.contains("commit"), "{combined}");
    // No cache file was created.
    assert!(
        !cwd.join(".tolkin/mcp-manifests").exists(),
        "CI refusal must happen before any disk write"
    );
}

#[test]
fn stdio_probe_against_python_fixture_when_available() {
    let Some(python) = find_python3() else {
        eprintln!("python3 not on PATH; skipping stdio fixture test");
        return;
    };
    let cwd = tmp("stdio-cwd");
    let data = tmp("stdio-data");
    let home = tmp("stdio-home");

    // Minimal MCP server fixture: reads JSON-RPC lines from stdin, replies
    // to initialize and tools/list on stdout. Ignores notifications.
    let script = cwd.join("server.py");
    fs::write(&script, PY_FIXTURE).unwrap();

    let config_path = cwd.join("mcp.json");
    let cmd_str = python;
    let script_str = script.to_string_lossy().to_string();
    fs::write(
        &config_path,
        format!(
            r#"{{ "mcpServers": {{ "tracker": {{ "command": "{cmd_str}", "args": ["{script_str}"] }} }} }}"#
        ),
    )
    .unwrap();

    let (stdout, stderr, ok) = run_mcp(
        &cwd,
        &data,
        &home,
        &[
            config_path.to_str().unwrap(),
            "--probe",
            "tracker",
            "--probe-timeout",
            "5",
        ],
        &[],
    );
    assert!(
        ok,
        "stdio probe run failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    );
    let cache_path = cwd.join(".tolkin/mcp-manifests/tracker.json");
    assert!(cache_path.exists(), "stdio cache file missing");
    let cache: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cache_path).unwrap()).unwrap();
    assert_eq!(cache["transport"].as_str(), Some("stdio"));
    assert!(cache["tools"].as_array().map(|a| a.len()).unwrap_or(0) >= 1);
}

fn find_python3() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("python3");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

const PY_FIXTURE: &str = r#"#!/usr/bin/env python3
import json, sys

def write(msg):
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        req = json.loads(line)
    except Exception:
        continue
    method = req.get("method", "")
    if method == "initialize":
        write({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "py-fixture", "version": "0.0.1"}
            }
        })
    elif method == "tools/list":
        write({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "tools": [
                    {"name": "ping", "description": "Ping the server. Returns pong.",
                     "inputSchema": {"type": "object"}}
                ]
            }
        })
    elif method.startswith("notifications/"):
        continue
    else:
        write({"jsonrpc": "2.0", "id": req.get("id"),
               "error": {"code": -32601, "message": "unknown"}})
"#;
