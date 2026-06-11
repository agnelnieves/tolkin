//! Opt-in sidecar client for OpenAI-compatible local LLM servers.
//!
//! Tolkin never talks to a cloud model on its own. This module discovers a
//! loopback server (mlx-lm, Ollama, LM Studio, llama-server) and exposes a
//! narrow chat client the optimize command will consume next wave.
//!
//! Loopback enforcement is hard: parsed host must be 127.0.0.1, ::1, or
//! localhost, unless TOLKIN_SIDECAR_ALLOW_REMOTE=1 explicitly opts out.
//!
//! The chat client picks read timeouts to fit slow MLX prefill on large
//! prompts (up to fifteen minutes) while keeping the connect timeout tight
//! so a missing server fails fast.
//!
//! No content here ships to the network on its own. detect() probes only
//! loopback; chat() only fires when the optimize command calls it.

// Optimize command lands next wave; allow until then so clippy stays clean.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use serde_json::Value;

/// Identified server family behind the OpenAI-compatible endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    MlxLm,
    Ollama,
    LmStudio,
    LlamaServer,
    Unknown,
}

impl Backend {
    pub fn label(&self) -> &'static str {
        match self {
            Backend::MlxLm => "mlx-lm",
            Backend::Ollama => "ollama",
            Backend::LmStudio => "lm-studio",
            Backend::LlamaServer => "llama-server",
            Backend::Unknown => "unknown",
        }
    }

    /// True when the backend honors OpenAI's response_format: json_schema
    /// with strict: true. mlx-lm has no upstream support, and Ollama's MLX
    /// path silently ignores it, so both return false here.
    pub fn supports_enforced_schema(&self) -> bool {
        matches!(self, Backend::LmStudio | Backend::LlamaServer)
    }
}

/// A resolved loopback server the optimize command can talk to.
#[derive(Clone, Debug)]
pub struct Sidecar {
    pub base_url: String,
    pub model: String,
    pub backend: Backend,
}

/// Per-endpoint result captured during detection.
#[derive(Clone, Debug)]
pub struct ProbeOutcome {
    pub url: String,
    pub reachable: bool,
    pub backend: Backend,
    pub models: Vec<String>,
    pub error: Option<String>,
}

/// Aggregate detection report. sidecar is Some when a usable endpoint was
/// found; probed records every endpoint attempted (or none, when the
/// TOLKIN_NO_SIDECAR=1 escape hatch is set).
#[derive(Clone, Debug, Default)]
pub struct ProbeReport {
    pub sidecar: Option<Sidecar>,
    pub probed: Vec<ProbeOutcome>,
    /// Set when an explicit model id was given but not present in /v1/models.
    pub model_warning: Option<String>,
}

/// Default probe order. Picked to match the upstream defaults: mlx-lm and
/// llama-server both ship on 8080, Ollama on 11434, LM Studio on 1234.
const DEFAULT_PROBES: &[&str] = &[
    "http://127.0.0.1:8080",
    "http://127.0.0.1:11434",
    "http://127.0.0.1:1234",
];

/// Returns true when the user has opted into a non-loopback sidecar via
/// TOLKIN_SIDECAR_ALLOW_REMOTE=1. Callers should print a warning in that
/// case so the choice is visible at every invocation.
pub fn remote_override_active() -> bool {
    matches!(std::env::var("TOLKIN_SIDECAR_ALLOW_REMOTE"), Ok(v) if v == "1")
}

/// Returns true when the user has disabled the sidecar entirely via
/// TOLKIN_NO_SIDECAR=1. detect() returns an empty report in that case.
pub fn disabled_by_env() -> bool {
    matches!(std::env::var("TOLKIN_NO_SIDECAR"), Ok(v) if v == "1")
}

/// Parse base_url and confirm its host is a loopback address.
///
/// The parser is hand-rolled to avoid a url crate dep. It accepts
/// scheme://host[:port][/...] and pulls the host slice, including the
/// bracketed [::1] form for v6 literals. Any non-loopback host returns
/// Err with a single-line explanation, unless the remote-override env is
/// set in which case Ok(()) is returned and the caller is expected to warn.
pub fn enforce_loopback(base_url: &str) -> Result<(), String> {
    let host = parse_host(base_url)
        .ok_or_else(|| format!("could not parse host from base_url '{base_url}'"))?;
    if is_loopback_host(&host) {
        return Ok(());
    }
    if remote_override_active() {
        return Ok(());
    }
    Err(format!(
        "sidecar host '{host}' is not loopback; set TOLKIN_SIDECAR_ALLOW_REMOTE=1 to override"
    ))
}

/// Extract the host portion of a base URL without a URL crate. Returns
/// None when the input is malformed. The returned string is lowercased
/// for case-insensitive host matching (localhost vs LOCALHOST).
fn parse_host(base_url: &str) -> Option<String> {
    let after_scheme = base_url.split_once("://")?.1;
    // Strip any path/query/fragment.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // Strip userinfo if present (user:pass@host).
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if authority.is_empty() {
        return None;
    }
    // IPv6 literal in brackets: keep the inner host but match it
    // case-insensitively below.
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, _) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        return Some(host.to_ascii_lowercase());
    }
    // host[:port]
    let host = authority.split(':').next().unwrap_or(authority);
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// True for the three loopback names tolkin accepts.
fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// Discover a loopback sidecar.
///
/// If TOLKIN_NO_SIDECAR=1, returns an empty report with sidecar None and
/// probed empty.
///
/// If explicit_base_url is Some, that URL is the only one probed (after
/// enforce_loopback). Otherwise the default loopback probe order runs.
/// First reachable endpoint with at least one model wins.
///
/// Model selection: explicit_model wins when present in the listed ids; if
/// it is not present, the report records a warning and the explicit model
/// is still used (the user knows their server). Otherwise the first
/// listed model id is taken.
pub fn detect(explicit_base_url: Option<&str>, explicit_model: Option<&str>) -> ProbeReport {
    let mut report = ProbeReport::default();
    if disabled_by_env() {
        return report;
    }

    let targets: Vec<String> = match explicit_base_url {
        Some(url) => {
            if let Err(e) = enforce_loopback(url) {
                report.probed.push(ProbeOutcome {
                    url: url.to_string(),
                    reachable: false,
                    backend: Backend::Unknown,
                    models: Vec::new(),
                    error: Some(e),
                });
                return report;
            }
            vec![url.trim_end_matches('/').to_string()]
        }
        None => DEFAULT_PROBES.iter().map(|s| (*s).to_string()).collect(),
    };

    for url in targets {
        let outcome = probe_endpoint(&url);
        let chosen = outcome.reachable && !outcome.models.is_empty();
        report.probed.push(outcome.clone());
        if chosen {
            let (model, warning) = select_model(&outcome.models, explicit_model);
            report.model_warning = warning;
            report.sidecar = Some(Sidecar {
                base_url: outcome.url,
                model,
                backend: outcome.backend,
            });
            break;
        }
    }
    report
}

fn select_model(listed: &[String], explicit_model: Option<&str>) -> (String, Option<String>) {
    if let Some(want) = explicit_model {
        if listed.iter().any(|m| m == want) {
            return (want.to_string(), None);
        }
        let warning = format!("model '{want}' not present in /v1/models response; using it anyway");
        return (want.to_string(), Some(warning));
    }
    // Reachable + non-empty guard already passed; index 0 is safe.
    (listed[0].clone(), None)
}

/// Probe a single base URL: GET {base}/v1/models, then refine the backend
/// guess. Port number is the first signal; for 8080 we try /props to
/// distinguish llama-server from mlx-lm. Failures populate error and leave
/// reachable=false.
fn probe_endpoint(base_url: &str) -> ProbeOutcome {
    let trimmed = base_url.trim_end_matches('/').to_string();
    let agent = build_probe_agent();
    let url = format!("{trimmed}/v1/models");
    let resp = agent.get(&url).call();
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            return ProbeOutcome {
                url: trimmed,
                reachable: false,
                backend: Backend::Unknown,
                models: Vec::new(),
                error: Some(probe_error(&e)),
            };
        }
    };
    let json: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => {
            return ProbeOutcome {
                url: trimmed,
                reachable: false,
                backend: Backend::Unknown,
                models: Vec::new(),
                error: Some(format!("malformed /v1/models JSON: {e}")),
            };
        }
    };
    let models = extract_model_ids(&json);
    let port = port_from_url(&trimmed).unwrap_or(0);
    let backend = identify_backend(port, |full_url| props_status(&agent, full_url), &trimmed);
    ProbeOutcome {
        url: trimmed,
        reachable: true,
        backend,
        models,
        error: None,
    }
}

/// Extract model ids from the OpenAI-shaped {"data": [{"id": "..."}]}
/// envelope. Ollama returns the same envelope through its /v1/models
/// compatibility shim, so this works across all four backends.
fn extract_model_ids(json: &serde_json::Value) -> Vec<String> {
    let Some(data) = json.get("data").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
        })
        .collect()
}

/// Pure backend identification, factored so it can be unit tested without
/// touching the network. `props_probe` is invoked only for port 8080: it
/// returns Some(true) when /props came back with usable JSON (llama-server),
/// Some(false) when /props returned a non-2xx (mlx-lm best guess), or None
/// on transport failure (also treated as mlx-lm best guess).
fn identify_backend<F>(port: u16, mut props_probe: F, trimmed_base: &str) -> Backend
where
    F: FnMut(&str) -> Option<bool>,
{
    match port {
        11434 => Backend::Ollama,
        1234 => Backend::LmStudio,
        8080 => {
            let url = format!("{trimmed_base}/props");
            match props_probe(&url) {
                Some(true) => Backend::LlamaServer,
                _ => Backend::MlxLm,
            }
        }
        _ => Backend::Unknown,
    }
}

/// Issue GET {url} and decide whether the response looks like a
/// llama-server /props payload. Some(true) on 2xx with parseable JSON,
/// Some(false) on a non-2xx status, None on transport failure or unparseable
/// JSON.
fn props_status(agent: &ureq::Agent, url: &str) -> Option<bool> {
    match agent.get(url).call() {
        Ok(r) => match r.into_json::<serde_json::Value>() {
            Ok(_) => Some(true),
            Err(_) => None,
        },
        Err(ureq::Error::Status(_, _)) => Some(false),
        Err(_) => None,
    }
}

fn port_from_url(base_url: &str) -> Option<u16> {
    let after_scheme = base_url.split_once("://")?.1;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (_, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (_h, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':').unwrap_or("");
        ("", port)
    } else {
        let mut iter = authority.rsplitn(2, ':');
        let port = iter.next().unwrap_or("");
        let host = iter.next().unwrap_or("");
        (host, port)
    };
    port.parse().ok()
}

fn build_probe_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(300))
        .timeout(Duration::from_millis(700))
        .build()
}

fn probe_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("HTTP {code} from sidecar"),
        ureq::Error::Transport(t) => {
            let msg = t.to_string();
            if msg.contains("Connection refused") || msg.contains("connection refused") {
                "connection refused (sidecar not running on this port)".to_string()
            } else {
                format!("transport error: {msg}")
            }
        }
    }
}

/// Borrowed-input chat request. Holding &str (and an owned Value when the
/// schema is set) keeps allocation under the caller's control.
#[derive(Clone, Debug)]
pub struct ChatRequest<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
    pub temperature: f32,
    pub json_schema: Option<Value>,
}

/// OpenAI finish_reason mirror with a catch-all for backend-specific values
/// (Ollama emits "load", llama-server occasionally "error", etc.).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    Other(String),
}

impl FinishReason {
    fn from_str(s: &str) -> Self {
        match s {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            other => FinishReason::Other(other.to_string()),
        }
    }
}

/// Successful chat result. prompt_tokens and completion_tokens are
/// optional because not every backend emits a usage block.
#[derive(Clone, Debug)]
pub struct ChatResponse {
    pub text: String,
    pub finish_reason: FinishReason,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub elapsed_ms: u128,
}

impl Sidecar {
    /// POST {base}/v1/chat/completions with the request encoded as the
    /// OpenAI Chat Completions schema. response_format is only attached
    /// when the backend has been verified to honor enforced JSON schema.
    ///
    /// Connect timeout is one second so a dead server fails fast; the
    /// overall timeout is set to fifteen minutes because MLX prefill on
    /// long prompts is genuinely that slow on Apple silicon.
    pub fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(1))
            .timeout(Duration::from_secs(15 * 60))
            .build();
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let body = build_chat_body(&self.model, self.backend, req);
        let started = Instant::now();
        let resp = agent
            .post(&url)
            .set("content-type", "application/json")
            .send_json(body);
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let detail = r.into_string().unwrap_or_default();
                return Err(format!("chat HTTP {code}: {detail}"));
            }
            Err(ureq::Error::Transport(t)) => {
                let msg = t.to_string();
                if msg.contains("Connection refused") || msg.contains("connection refused") {
                    return Err(
                        "sidecar refused the chat request: the server is no longer running"
                            .to_string(),
                    );
                }
                return Err(format!("chat transport error: {msg}"));
            }
        };
        let elapsed_ms = started.elapsed().as_millis();
        let json: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("chat response was not JSON: {e}"))?;
        parse_chat_response(&json, elapsed_ms)
    }
}

fn build_chat_body(model: &str, backend: Backend, req: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": req.system },
            { "role": "user", "content": req.user },
        ],
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "stream": false,
    });
    if let Some(schema) = req.json_schema.as_ref() {
        if backend.supports_enforced_schema() {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "tolkin",
                    "schema": schema,
                    "strict": true,
                },
            });
        }
    }
    body
}

fn parse_chat_response(json: &serde_json::Value, elapsed_ms: u128) -> Result<ChatResponse, String> {
    let choice = json
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "chat response missing choices[0]".to_string())?;
    let text = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(FinishReason::from_str)
        .unwrap_or_else(|| FinishReason::Other(String::new()));
    let usage = json.get("usage");
    let prompt_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(serde_json::Value::as_u64);
    let completion_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(serde_json::Value::as_u64);
    Ok(ChatResponse {
        text,
        finish_reason,
        prompt_tokens,
        completion_tokens,
        elapsed_ms,
    })
}

/// True when the model ran out of room. Either the explicit Length reason
/// or completion_tokens hitting the budget counts. Callers should retry
/// with a higher cap or a shorter prompt.
pub fn truncated(resp: &ChatResponse, max_tokens: u32) -> bool {
    if resp.finish_reason == FinishReason::Length {
        return true;
    }
    matches!(resp.completion_tokens, Some(c) if c >= u64::from(max_tokens))
}

/// Pull a JSON object out of raw model output.
///
/// Handles three cases:
///   1. Fenced ```json ... ``` or ``` ... ``` block.
///   2. Prose wrapped output where the first '{' starts the JSON.
///   3. Raw JSON.
///
/// Returns None when no balanced object can be located. String contents
/// (including escaped quotes and backslashes) are respected during brace
/// counting so embedded '{' and '}' do not break the balance.
pub fn extract_json_object(text: &str) -> Option<String> {
    let candidate = strip_fences(text);
    let bytes = candidate.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut idx = start;
    while idx < bytes.len() {
        let b = bytes[idx];
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(candidate[start..=idx].to_string());
                    }
                }
                _ => {}
            }
        }
        idx += 1;
    }
    None
}

/// Strip the outermost ```json ... ``` or ``` ... ``` fence pair if both
/// markers are present. Anything outside the fences (prose, headings) is
/// discarded. When no closing fence is found the input is returned as is
/// so the brace scanner can still attempt to recover a partial object.
fn strip_fences(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Drop the optional language tag like "json".
        let body = rest.split_once('\n').map_or(rest, |(_lang, b)| b);
        if let Some(end) = body.rfind("```") {
            return body[..end].to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_labels_are_stable() {
        assert_eq!(Backend::MlxLm.label(), "mlx-lm");
        assert_eq!(Backend::Ollama.label(), "ollama");
        assert_eq!(Backend::LmStudio.label(), "lm-studio");
        assert_eq!(Backend::LlamaServer.label(), "llama-server");
        assert_eq!(Backend::Unknown.label(), "unknown");
    }

    #[test]
    fn only_lmstudio_and_llamaserver_enforce_schema() {
        assert!(!Backend::MlxLm.supports_enforced_schema());
        assert!(!Backend::Ollama.supports_enforced_schema());
        assert!(Backend::LmStudio.supports_enforced_schema());
        assert!(Backend::LlamaServer.supports_enforced_schema());
        assert!(!Backend::Unknown.supports_enforced_schema());
    }

    #[test]
    fn parse_host_accepts_v4_v6_named() {
        assert_eq!(
            parse_host("http://127.0.0.1:8080").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(parse_host("http://[::1]:1234/v1").as_deref(), Some("::1"));
        assert_eq!(
            parse_host("http://localhost:11434/").as_deref(),
            Some("localhost")
        );
        assert_eq!(
            parse_host("http://LOCALHOST/").as_deref(),
            Some("localhost")
        );
        assert_eq!(
            parse_host("http://user:pass@127.0.0.1:8080/path").as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn parse_host_rejects_malformed_input() {
        assert!(parse_host("not a url").is_none());
        assert!(parse_host("http://").is_none());
        assert!(parse_host("http://[]:8080").is_none());
    }

    /// All env-sensitive enforce_loopback assertions live in one test so they
    /// share a single mutating window over the process-wide env var. Cargo
    /// runs tests in parallel by default; splitting these into separate
    /// #[test] functions caused intermittent failures when one test removed
    /// the variable while another set it.
    #[test]
    fn enforce_loopback_env_matrix() {
        let _guard = env_lock();
        let prev = std::env::var("TOLKIN_SIDECAR_ALLOW_REMOTE").ok();

        // Loopback hosts pass with the override unset.
        std::env::remove_var("TOLKIN_SIDECAR_ALLOW_REMOTE");
        assert!(enforce_loopback("http://127.0.0.1:8080").is_ok());
        assert!(enforce_loopback("http://[::1]:8080").is_ok());
        assert!(enforce_loopback("http://localhost:8080").is_ok());

        // Remote hosts are rejected with a hint pointing at the override.
        let err = enforce_loopback("http://10.0.0.5:8080").unwrap_err();
        assert!(err.contains("not loopback"));
        assert!(err.contains("TOLKIN_SIDECAR_ALLOW_REMOTE"));

        // Malformed URLs surface a parse error regardless of the override.
        assert!(enforce_loopback("not-a-url").is_err());

        // With the override set, even non-loopback hosts pass.
        std::env::set_var("TOLKIN_SIDECAR_ALLOW_REMOTE", "1");
        assert!(enforce_loopback("http://10.0.0.5:8080").is_ok());
        assert!(remote_override_active());

        restore_env("TOLKIN_SIDECAR_ALLOW_REMOTE", prev);
    }

    #[test]
    fn identify_backend_by_port() {
        let never_called = |_url: &str| panic!("props probe should not run on ollama port");
        assert_eq!(
            identify_backend(11434, never_called, "http://127.0.0.1:11434"),
            Backend::Ollama
        );
        let never_called = |_url: &str| panic!("props probe should not run on lmstudio port");
        assert_eq!(
            identify_backend(1234, never_called, "http://127.0.0.1:1234"),
            Backend::LmStudio
        );
    }

    #[test]
    fn identify_backend_unknown_port_is_unknown() {
        let never_called = |_url: &str| panic!("props probe should not run on unknown port");
        assert_eq!(
            identify_backend(9999, never_called, "http://127.0.0.1:9999"),
            Backend::Unknown
        );
    }

    #[test]
    fn identify_backend_8080_uses_props_probe() {
        let llama = |url: &str| {
            assert!(url.ends_with("/props"));
            Some(true)
        };
        assert_eq!(
            identify_backend(8080, llama, "http://127.0.0.1:8080"),
            Backend::LlamaServer
        );
        let mlx_404 = |_url: &str| Some(false);
        assert_eq!(
            identify_backend(8080, mlx_404, "http://127.0.0.1:8080"),
            Backend::MlxLm
        );
        let mlx_refused = |_url: &str| None;
        assert_eq!(
            identify_backend(8080, mlx_refused, "http://127.0.0.1:8080"),
            Backend::MlxLm
        );
    }

    #[test]
    fn port_from_url_handles_all_forms() {
        assert_eq!(port_from_url("http://127.0.0.1:8080"), Some(8080));
        assert_eq!(port_from_url("http://[::1]:11434/v1"), Some(11434));
        assert_eq!(port_from_url("http://localhost:1234/"), Some(1234));
        assert_eq!(port_from_url("http://localhost"), None);
    }

    #[test]
    fn extract_model_ids_pulls_data_array() {
        let json = serde_json::json!({
            "object": "list",
            "data": [
                { "id": "llama-3-8b" },
                { "id": "qwen-0.6b" },
                { "id": 42 },
            ],
        });
        assert_eq!(
            extract_model_ids(&json),
            vec!["llama-3-8b".to_string(), "qwen-0.6b".to_string()]
        );
    }

    #[test]
    fn extract_model_ids_empty_when_no_data() {
        let json = serde_json::json!({ "error": "nope" });
        assert!(extract_model_ids(&json).is_empty());
    }

    #[test]
    fn select_model_prefers_explicit_when_present() {
        let listed = vec!["a".to_string(), "b".to_string()];
        let (model, warning) = select_model(&listed, Some("b"));
        assert_eq!(model, "b");
        assert!(warning.is_none());
    }

    #[test]
    fn select_model_warns_when_explicit_missing() {
        let listed = vec!["a".to_string()];
        let (model, warning) = select_model(&listed, Some("missing"));
        assert_eq!(model, "missing");
        assert!(warning.as_deref().unwrap().contains("missing"));
    }

    #[test]
    fn select_model_falls_back_to_first_listed() {
        let listed = vec!["first".to_string(), "second".to_string()];
        let (model, warning) = select_model(&listed, None);
        assert_eq!(model, "first");
        assert!(warning.is_none());
    }

    #[test]
    fn build_chat_body_omits_response_format_when_no_schema() {
        let req = ChatRequest {
            system: "s",
            user: "u",
            max_tokens: 64,
            temperature: 0.1,
            json_schema: None,
        };
        let body = build_chat_body("m", Backend::LmStudio, &req);
        assert!(body.get("response_format").is_none());
        assert_eq!(body["stream"], serde_json::Value::Bool(false));
        assert_eq!(body["model"], "m");
    }

    #[test]
    fn build_chat_body_includes_schema_for_supported_backends() {
        let schema = serde_json::json!({ "type": "object" });
        let req = ChatRequest {
            system: "s",
            user: "u",
            max_tokens: 64,
            temperature: 0.0,
            json_schema: Some(schema.clone()),
        };
        for backend in [Backend::LmStudio, Backend::LlamaServer] {
            let body = build_chat_body("m", backend, &req);
            let rf = body.get("response_format").expect("response_format set");
            assert_eq!(rf["type"], "json_schema");
            assert_eq!(rf["json_schema"]["name"], "tolkin");
            assert_eq!(rf["json_schema"]["strict"], serde_json::Value::Bool(true));
            assert_eq!(rf["json_schema"]["schema"], schema);
        }
    }

    #[test]
    fn build_chat_body_drops_schema_for_unsupported_backends() {
        let schema = serde_json::json!({ "type": "object" });
        let req = ChatRequest {
            system: "s",
            user: "u",
            max_tokens: 64,
            temperature: 0.0,
            json_schema: Some(schema),
        };
        for backend in [Backend::MlxLm, Backend::Ollama, Backend::Unknown] {
            let body = build_chat_body("m", backend, &req);
            assert!(body.get("response_format").is_none(), "{}", backend.label());
        }
    }

    #[test]
    fn parse_chat_response_reads_text_and_usage() {
        let json = serde_json::json!({
            "choices": [{
                "message": { "content": "hello" },
                "finish_reason": "stop",
            }],
            "usage": { "prompt_tokens": 12, "completion_tokens": 4 },
        });
        let resp = parse_chat_response(&json, 42).unwrap();
        assert_eq!(resp.text, "hello");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.prompt_tokens, Some(12));
        assert_eq!(resp.completion_tokens, Some(4));
        assert_eq!(resp.elapsed_ms, 42);
    }

    #[test]
    fn parse_chat_response_maps_unknown_finish_reason() {
        let json = serde_json::json!({
            "choices": [{
                "message": { "content": "" },
                "finish_reason": "tool_calls",
            }],
        });
        let resp = parse_chat_response(&json, 1).unwrap();
        assert_eq!(
            resp.finish_reason,
            FinishReason::Other("tool_calls".to_string())
        );
        assert!(resp.prompt_tokens.is_none());
    }

    #[test]
    fn parse_chat_response_errors_without_choices() {
        let json = serde_json::json!({ "error": "no choices" });
        assert!(parse_chat_response(&json, 0).is_err());
    }

    #[test]
    fn truncated_detects_length_finish() {
        let resp = ChatResponse {
            text: String::new(),
            finish_reason: FinishReason::Length,
            prompt_tokens: None,
            completion_tokens: None,
            elapsed_ms: 0,
        };
        assert!(truncated(&resp, 100));
    }

    #[test]
    fn truncated_detects_completion_at_budget() {
        let resp = ChatResponse {
            text: String::new(),
            finish_reason: FinishReason::Stop,
            prompt_tokens: None,
            completion_tokens: Some(100),
            elapsed_ms: 0,
        };
        assert!(truncated(&resp, 100));
    }

    #[test]
    fn truncated_false_when_under_budget() {
        let resp = ChatResponse {
            text: String::new(),
            finish_reason: FinishReason::Stop,
            prompt_tokens: None,
            completion_tokens: Some(50),
            elapsed_ms: 0,
        };
        assert!(!truncated(&resp, 100));
    }

    #[test]
    fn extract_json_object_unwraps_fenced_json_block() {
        let text = "Here you go:\n```json\n{\"a\": 1, \"b\": [1, 2]}\n```";
        let got = extract_json_object(text).unwrap();
        assert_eq!(got, "{\"a\": 1, \"b\": [1, 2]}");
    }

    #[test]
    fn extract_json_object_unwraps_unlabeled_fence() {
        let text = "```\n{\"k\": \"v\"}\n```";
        let got = extract_json_object(text).unwrap();
        assert_eq!(got, "{\"k\": \"v\"}");
    }

    #[test]
    fn extract_json_object_returns_raw_object() {
        let text = "{\"x\": 1}";
        assert_eq!(extract_json_object(text).as_deref(), Some("{\"x\": 1}"));
    }

    #[test]
    fn extract_json_object_pulls_from_prose() {
        let text = "Here is a response. The JSON: {\"verdict\": \"keep\"} thanks!";
        assert_eq!(
            extract_json_object(text).as_deref(),
            Some("{\"verdict\": \"keep\"}")
        );
    }

    #[test]
    fn extract_json_object_ignores_braces_inside_strings() {
        let text = "{\"msg\": \"weird }} value\", \"ok\": true}";
        assert_eq!(extract_json_object(text), Some(text.to_string()));
    }

    #[test]
    fn extract_json_object_handles_escaped_quotes() {
        let text = "{\"s\": \"a \\\"quoted\\\" }\", \"n\": 1}";
        assert_eq!(extract_json_object(text), Some(text.to_string()));
    }

    #[test]
    fn extract_json_object_handles_nested_objects() {
        let text = "prefix {\"outer\": {\"inner\": [1, {\"k\": 2}]}} trailing";
        assert_eq!(
            extract_json_object(text).as_deref(),
            Some("{\"outer\": {\"inner\": [1, {\"k\": 2}]}}")
        );
    }

    #[test]
    fn extract_json_object_returns_none_when_unbalanced() {
        let text = "broken: {\"a\": 1, \"b\": {\"c\": 2}";
        assert!(extract_json_object(text).is_none());
    }

    #[test]
    fn extract_json_object_returns_none_without_object() {
        assert!(extract_json_object("no braces here").is_none());
        assert!(extract_json_object("").is_none());
    }

    /// Best-effort env restore for tests that twiddle the override flag.
    /// Tests run on a single Cargo test binary by default; using a single
    /// process-wide variable is the documented pattern in std.
    fn restore_env(key: &str, prev: Option<String>) {
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    /// Shared mutex any env-mutating test must lock. Cargo runs tests in
    /// parallel by default and env vars are process-wide; serializing the
    /// env tests is the only way to keep them deterministic without a new
    /// crate dep.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        // A poisoned guard still serializes access; recover it explicitly.
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn disabled_by_env_returns_empty_report() {
        let _guard = env_lock();
        let prev = std::env::var("TOLKIN_NO_SIDECAR").ok();
        std::env::set_var("TOLKIN_NO_SIDECAR", "1");
        assert!(disabled_by_env());
        let report = detect(None, None);
        assert!(report.sidecar.is_none());
        assert!(report.probed.is_empty());
        restore_env("TOLKIN_NO_SIDECAR", prev);
    }

    #[test]
    fn detect_records_loopback_violation_for_explicit_remote() {
        let _guard = env_lock();
        let prev_remote = std::env::var("TOLKIN_SIDECAR_ALLOW_REMOTE").ok();
        let prev_disable = std::env::var("TOLKIN_NO_SIDECAR").ok();
        std::env::remove_var("TOLKIN_SIDECAR_ALLOW_REMOTE");
        std::env::remove_var("TOLKIN_NO_SIDECAR");
        let report = detect(Some("http://10.0.0.5:8080"), None);
        assert!(report.sidecar.is_none());
        assert_eq!(report.probed.len(), 1);
        let outcome = &report.probed[0];
        assert!(!outcome.reachable);
        assert_eq!(outcome.backend, Backend::Unknown);
        assert!(outcome.error.as_deref().unwrap().contains("not loopback"));
        restore_env("TOLKIN_SIDECAR_ALLOW_REMOTE", prev_remote);
        restore_env("TOLKIN_NO_SIDECAR", prev_disable);
    }
}
