//! Minimal MCP client for `tolkin mcp --probe`. Speaks JSON-RPC 2.0 over
//! stdio (newline-delimited) and Streamable HTTP (POST with optional SSE
//! response). Zero new dependencies: serde_json, std::process, and the
//! in-tree ureq 2.
//!
//! Scope is deliberate: this is a one-shot probe that runs initialize +
//! tools/list and exits. It is NOT a long-running client. It will never
//! issue tools/call, never subscribe to notifications beyond what the spec
//! requires to read the initialize response, never spawn anything not
//! already configured in the user's own MCP config.
//!
//! Privacy posture (binding): callers MUST gate this behind interactive
//! consent and the CI refusal in [`crate::commands::mcp`]. The probe itself
//! does not consent; it just runs.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

const DEFAULT_TIMEOUT_SECS: u64 = 10;
const MAX_PAGES: usize = 32;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const PROTOCOL_VERSION: &str = "2025-03-26";

/// Outcome of a successful probe: the raw tool objects plus the negotiated
/// protocol version when the server reported one.
#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub tools: Vec<Value>,
    pub protocol_version: Option<String>,
}

/// Stdio transport configuration: the exact command + args + env that will
/// be spawned. Env merges over the inherited environment.
#[derive(Clone, Debug)]
pub struct StdioTarget {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// HTTP transport configuration: the URL and the headers to send. Headers
/// are merged with the analyzer-supplied Accept / Content-Type defaults.
#[derive(Clone, Debug)]
pub struct HttpTarget {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

/// A confirmation-friendly one-liner describing the exact stdio command
/// that will run, with env VARIABLE NAMES ONLY (never values). This is the
/// string the CLI shows the user before asking y/N.
pub fn stdio_confirmation_line(t: &StdioTarget) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !t.env.is_empty() {
        parts.push(format!(
            "env({})",
            t.env.keys().cloned().collect::<Vec<_>>().join(",")
        ));
    }
    parts.push(t.command.clone());
    for a in &t.args {
        parts.push(a.clone());
    }
    parts.join(" ")
}

/// The HTTP variant: the URL plus the names (not values) of headers that
/// will be sent on top of the analyzer defaults.
pub fn http_confirmation_line(t: &HttpTarget) -> String {
    if t.headers.is_empty() {
        format!("POST {} (default headers only)", t.url)
    } else {
        let names: Vec<String> = t.headers.keys().cloned().collect();
        format!("POST {} (extra headers: {})", t.url, names.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (tested in unit tests)
// ---------------------------------------------------------------------------

/// Encode a single JSON-RPC message as a newline-terminated UTF-8 line.
/// MCP stdio framing is newline-delimited; this is the on-the-wire form.
pub fn encode_frame(value: &Value) -> Vec<u8> {
    let mut buf = serde_json::to_vec(value).unwrap_or_default();
    buf.push(b'\n');
    buf
}

/// Parse one line into a JSON value, ignoring leading/trailing whitespace.
/// Returns None on an empty line so callers can skip blanks cleanly.
pub fn decode_frame(line: &str) -> Option<Value> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    serde_json::from_str(s).ok()
}

/// True when `msg` is the JSON-RPC response to `expected_id`. A response
/// carries `result` or `error` plus an id that matches.
pub fn is_response_for(msg: &Value, expected_id: u64) -> bool {
    let id_matches = msg
        .get("id")
        .and_then(|v| v.as_u64())
        .map(|id| id == expected_id)
        .unwrap_or(false);
    let is_response = msg.get("result").is_some() || msg.get("error").is_some();
    id_matches && is_response
}

/// Merge a tools/list page result into a running accumulator. The page
/// shape is `{"tools": [...], "nextCursor": "..."}`. Returns the cursor
/// for the next page (None when no more pages).
pub fn merge_tools_page(acc: &mut Vec<Value>, page: &Value) -> Option<String> {
    if let Some(arr) = page.get("tools").and_then(Value::as_array) {
        acc.extend(arr.iter().cloned());
    }
    page.get("nextCursor")
        .and_then(Value::as_str)
        .map(String::from)
}

/// Extract the JSON-RPC payload from an SSE `data:` line. Returns the JSON
/// substring or None when this line is not a data line.
pub fn sse_data_line(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("data:")?;
    Some(rest.trim_start())
}

/// Best-effort short snippet for an error message: the last `max` bytes of
/// the captured stderr, with trailing whitespace trimmed and any embedded
/// NULs dropped. Pure so the IO path can be tested.
pub fn tail_snippet(stderr: &[u8], max: usize) -> String {
    let start = stderr.len().saturating_sub(max);
    let slice = &stderr[start..];
    let s: String = String::from_utf8_lossy(slice)
        .chars()
        .filter(|c| *c != '\0')
        .collect();
    s.trim().to_string()
}

// ---------------------------------------------------------------------------
// stdio transport
// ---------------------------------------------------------------------------

/// Run the probe over stdio. Spawns the configured command, runs the MCP
/// handshake (initialize + initialized + tools/list), and returns the
/// accumulated tools. Always kills the child before returning (success or
/// failure) to avoid lingering processes.
pub fn probe_stdio(target: &StdioTarget, timeout_secs: u64) -> Result<ProbeResult> {
    let timeout = Duration::from_secs(timeout_secs.max(1));
    let deadline = Instant::now() + timeout;

    let mut cmd = Command::new(&target.command);
    cmd.args(&target.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &target.env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn {}", target.command))?;

    let result = run_stdio_session(&mut child, deadline);
    // Always tear the child down. Best-effort: if kill fails the OS will
    // reap on process exit anyway.
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn run_stdio_session(child: &mut Child, deadline: Instant) -> Result<ProbeResult> {
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no stdin handle on spawned server"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("no stdout handle on spawned server"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("no stderr handle on spawned server"))?;

    // The handshake reads on the calling thread, but stderr would block the
    // child if it filled the pipe buffer. Drain it on a side thread into a
    // shared buffer so we can surface the last bytes on error.
    let (stderr_buf, stderr_join) = drain_stderr(stderr);

    let session = StdioSession {
        stdin,
        stdout: BufReader::new(stdout),
        deadline,
        next_id: 1,
    };
    let outcome = session.run();

    // Stop draining: dropping the BufReader on the calling side closes the
    // pipe so the side thread will exit.
    let stderr_text = match stderr_join.join() {
        Ok(_) => {
            let guard = stderr_buf.lock().unwrap_or_else(|p| p.into_inner());
            tail_snippet(&guard, 500)
        }
        Err(_) => String::new(),
    };

    outcome.map_err(|e| {
        if stderr_text.is_empty() {
            e
        } else {
            anyhow!("{e}; server stderr tail: {stderr_text}")
        }
    })
}

struct StdioSession<R: Read, W: Write> {
    stdin: W,
    stdout: BufReader<R>,
    deadline: Instant,
    next_id: u64,
}

impl<R: Read, W: Write> StdioSession<R, W> {
    fn run(mut self) -> Result<ProbeResult> {
        // initialize.
        let init_id = self.send_request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "tolkin", "version": env!("CARGO_PKG_VERSION") },
            }),
        )?;
        let init_resp = self.read_response_for(init_id)?;
        let protocol_version = init_resp
            .get("result")
            .and_then(|r| r.get("protocolVersion"))
            .and_then(Value::as_str)
            .map(String::from);

        // notifications/initialized.
        self.send_notification("notifications/initialized", json!({}))?;

        // tools/list with cursor pagination.
        let mut tools: Vec<Value> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let id = self.send_request("tools/list", params)?;
            let resp = self.read_response_for(id)?;
            let result = resp
                .get("result")
                .ok_or_else(|| anyhow!("tools/list returned no result"))?;
            cursor = merge_tools_page(&mut tools, result);
            if cursor.is_none() {
                break;
            }
        }

        Ok(ProbeResult {
            tools,
            protocol_version,
        })
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_frame(&msg)?;
        Ok(id)
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_frame(&msg)
    }

    fn write_frame(&mut self, msg: &Value) -> Result<()> {
        let bytes = encode_frame(msg);
        self.stdin
            .write_all(&bytes)
            .context("write JSON-RPC frame to server stdin")?;
        self.stdin.flush().context("flush server stdin")
    }

    fn read_response_for(&mut self, id: u64) -> Result<Value> {
        loop {
            let line = self.read_line_with_deadline()?;
            let Some(msg) = decode_frame(&line) else {
                continue;
            };
            if is_response_for(&msg, id) {
                if let Some(err) = msg.get("error") {
                    bail!("JSON-RPC error: {err}");
                }
                return Ok(msg);
            }
            // Notifications and server-initiated requests are politely
            // ignored: tolkin is not a long-running client.
        }
    }

    fn read_line_with_deadline(&mut self) -> Result<String> {
        let mut buf = Vec::with_capacity(256);
        let mut last_check = Instant::now();
        loop {
            // Cheap deadline check between bytes; the read itself can still
            // block. The caller kills the child on overall failure.
            if Instant::now() >= self.deadline {
                bail!("probe timed out after waiting for a JSON-RPC response");
            }
            // BufReader::fill_buf would let us peek without consuming, but
            // it blocks just the same. The simplest portable approach: read
            // a small chunk at a time and break on newline.
            let mut byte = [0u8; 1];
            match self.stdout.read(&mut byte) {
                Ok(0) => bail!("server closed stdout before sending a JSON-RPC response"),
                Ok(_) => {
                    if byte[0] == b'\n' {
                        return Ok(String::from_utf8_lossy(&buf).into_owned());
                    }
                    buf.push(byte[0]);
                }
                Err(e) => bail!("read from server stdout: {e}"),
            }
            if last_check.elapsed() > Duration::from_millis(50) {
                last_check = Instant::now();
            }
        }
    }
}

fn drain_stderr<R: Read + Send + 'static>(
    stderr: R,
) -> (
    std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    std::thread::JoinHandle<()>,
) {
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::with_capacity(1024)));
    let buf_t = buf.clone();
    let handle = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut chunk = [0u8; 256];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = buf_t.lock() {
                        // Cap the captured stderr so a noisy server cannot
                        // balloon our memory; the tail is all we need.
                        if guard.len() < 16 * 1024 {
                            guard.extend_from_slice(&chunk[..n]);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
    (buf, handle)
}

// ---------------------------------------------------------------------------
// http transport (Streamable HTTP)
// ---------------------------------------------------------------------------

/// Run the probe over HTTP. POSTs JSON-RPC messages to the server URL and
/// reads either application/json or SSE responses. Honors a session-id
/// header if the server hands one back on initialize.
pub fn probe_http(target: &HttpTarget, timeout_secs: u64) -> Result<ProbeResult> {
    let timeout = Duration::from_secs(timeout_secs.max(1));
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout(timeout)
        .build();

    let mut session_id: Option<String> = None;

    // initialize.
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "tolkin", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let (init_resp, sid) = http_round_trip(&agent, target, &session_id, &init_req, 1)?;
    if sid.is_some() {
        session_id = sid;
    }
    let protocol_version = init_resp
        .get("result")
        .and_then(|r| r.get("protocolVersion"))
        .and_then(Value::as_str)
        .map(String::from);

    // notifications/initialized: best-effort, ignore failures because some
    // servers respond 202 with an empty body.
    let init_notif = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    let _ = http_post_notification(&agent, target, &session_id, &init_notif);

    // tools/list pages.
    let mut tools: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut next_id: u64 = 2;
    for _ in 0..MAX_PAGES {
        let params = match &cursor {
            Some(c) => json!({ "cursor": c }),
            None => json!({}),
        };
        let req = json!({
            "jsonrpc": "2.0",
            "id": next_id,
            "method": "tools/list",
            "params": params,
        });
        let (resp, _sid) = http_round_trip(&agent, target, &session_id, &req, next_id)?;
        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("tools/list returned no result"))?;
        cursor = merge_tools_page(&mut tools, result);
        next_id += 1;
        if cursor.is_none() {
            break;
        }
    }

    Ok(ProbeResult {
        tools,
        protocol_version,
    })
}

fn http_round_trip(
    agent: &ureq::Agent,
    target: &HttpTarget,
    session_id: &Option<String>,
    body: &Value,
    expected_id: u64,
) -> Result<(Value, Option<String>)> {
    let mut req = agent.post(&target.url);
    req = req
        .set("Accept", "application/json, text/event-stream")
        .set("Content-Type", "application/json");
    if let Some(sid) = session_id {
        req = req.set("Mcp-Session-Id", sid);
    }
    for (k, v) in &target.headers {
        req = req.set(k, v);
    }

    let resp = match req.send_json(body.clone()) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r.into_string().unwrap_or_default();
            let hint = if (400..500).contains(&code) {
                ". Hint: legacy SSE-only MCP servers are out of scope for v1; tolkin probe targets Streamable HTTP."
            } else {
                ""
            };
            bail!("HTTP {code} on initialize: {detail}{hint}");
        }
        Err(ureq::Error::Transport(t)) => {
            bail!("HTTP transport error: {t}");
        }
    };

    let sid = resp.header("Mcp-Session-Id").map(String::from);
    let content_type = resp
        .header("Content-Type")
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("text/event-stream") {
        let value = read_sse_matching(resp, expected_id)?;
        Ok((value, sid))
    } else {
        // Bounded read so a misbehaving server cannot exhaust memory.
        let mut reader = resp.into_reader().take(MAX_BODY_BYTES as u64);
        let mut buf = String::new();
        reader
            .read_to_string(&mut buf)
            .context("read HTTP response body")?;
        let value: Value = serde_json::from_str(buf.trim())
            .with_context(|| format!("parse JSON response: {buf}"))?;
        Ok((value, sid))
    }
}

fn http_post_notification(
    agent: &ureq::Agent,
    target: &HttpTarget,
    session_id: &Option<String>,
    body: &Value,
) -> Result<()> {
    let mut req = agent.post(&target.url);
    req = req
        .set("Accept", "application/json, text/event-stream")
        .set("Content-Type", "application/json");
    if let Some(sid) = session_id {
        req = req.set("Mcp-Session-Id", sid);
    }
    for (k, v) in &target.headers {
        req = req.set(k, v);
    }
    let _ = req.send_json(body.clone());
    Ok(())
}

fn read_sse_matching(resp: ureq::Response, expected_id: u64) -> Result<Value> {
    let reader = BufReader::new(resp.into_reader().take(MAX_BODY_BYTES as u64));
    for line in reader.lines() {
        let line = line.context("read SSE line")?;
        let Some(data) = sse_data_line(&line) else {
            continue;
        };
        let Ok(value): Result<Value, _> = serde_json::from_str(data) else {
            continue;
        };
        if is_response_for(&value, expected_id) {
            if let Some(err) = value.get("error") {
                bail!("JSON-RPC error: {err}");
            }
            return Ok(value);
        }
    }
    bail!("SSE stream ended before a matching JSON-RPC response was received");
}

/// Convenience default timeout, exposed for the CLI default value.
pub const fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let v = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
        let bytes = encode_frame(&v);
        assert!(bytes.ends_with(b"\n"));
        let line = std::str::from_utf8(&bytes).unwrap().trim_end();
        let decoded = decode_frame(line).unwrap();
        assert_eq!(decoded, v);
        assert!(decode_frame("").is_none());
        assert!(decode_frame("not json").is_none());
    }

    #[test]
    fn id_matching_distinguishes_responses_from_notifications() {
        let resp = json!({ "jsonrpc": "2.0", "id": 2, "result": {} });
        assert!(is_response_for(&resp, 2));
        assert!(!is_response_for(&resp, 3));

        // Notification: method + params, no id.
        let notif = json!({ "jsonrpc": "2.0", "method": "x", "params": {} });
        assert!(!is_response_for(&notif, 2));

        // Server-initiated request: has id and method but no result/error.
        let server_req = json!({ "jsonrpc": "2.0", "id": 2, "method": "sampling/createMessage" });
        assert!(!is_response_for(&server_req, 2));

        // Error response.
        let err = json!({ "jsonrpc": "2.0", "id": 2, "error": { "code": -1, "message": "x" } });
        assert!(is_response_for(&err, 2));
    }

    #[test]
    fn pagination_merge_collects_tools_and_returns_cursor() {
        let mut acc = Vec::new();
        let page1 = json!({ "tools": [ { "name": "a" }, { "name": "b" } ], "nextCursor": "c1" });
        let cursor = merge_tools_page(&mut acc, &page1);
        assert_eq!(cursor.as_deref(), Some("c1"));
        assert_eq!(acc.len(), 2);

        let page2 = json!({ "tools": [ { "name": "c" } ] });
        let cursor = merge_tools_page(&mut acc, &page2);
        assert!(cursor.is_none());
        assert_eq!(acc.len(), 3);
    }

    #[test]
    fn sse_data_line_extraction() {
        assert_eq!(sse_data_line("data: {\"id\":1}"), Some("{\"id\":1}"));
        assert_eq!(sse_data_line("  data:{\"id\":1}"), Some("{\"id\":1}"));
        assert_eq!(sse_data_line("event: message"), None);
        assert_eq!(sse_data_line(""), None);
        assert_eq!(sse_data_line(": comment"), None);
    }

    #[test]
    fn tail_snippet_keeps_last_bytes_and_trims() {
        let s = b"abc\0\n  noisy server output    ";
        let out = tail_snippet(s, 500);
        assert_eq!(out, "abc\n  noisy server output");
        let big = vec![b'x'; 1024];
        let tail = tail_snippet(&big, 32);
        assert_eq!(tail.len(), 32);
    }

    #[test]
    fn stdio_confirmation_lists_env_names_only() {
        let mut env = BTreeMap::new();
        env.insert("SECRET_TOKEN".to_string(), "topsecret".to_string());
        env.insert("ANOTHER".to_string(), "value".to_string());
        let t = StdioTarget {
            command: "node".to_string(),
            args: vec!["./srv.js".to_string()],
            env,
        };
        let line = stdio_confirmation_line(&t);
        assert!(line.contains("ANOTHER"));
        assert!(line.contains("SECRET_TOKEN"));
        assert!(!line.contains("topsecret"), "values must never appear");
        assert!(!line.contains("value"));
        assert!(line.contains("node"));
    }

    #[test]
    fn http_confirmation_lists_header_names_only() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer xyz".to_string());
        let t = HttpTarget {
            url: "https://example.test/mcp".to_string(),
            headers,
        };
        let line = http_confirmation_line(&t);
        assert!(line.contains("Authorization"));
        assert!(!line.contains("Bearer xyz"));
        assert!(line.contains("https://example.test/mcp"));
    }
}
