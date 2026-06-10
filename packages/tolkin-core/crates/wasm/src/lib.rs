//! WebAssembly bindings for tolkin-core. Each function takes/returns JSON
//! strings so the JS side gets plain objects without a serde-wasm-bindgen
//! dependency. The browser parses the results into the types mirrored in
//! `apps/tolkin-web/src/lib/core`.

use wasm_bindgen::prelude::*;

/// Crate version. Proves the wasm wiring end to end.
#[wasm_bindgen]
pub fn version() -> String {
    tolkin_core::version().to_string()
}

/// Redact secrets in `text`. `options_json` may be empty for defaults, or a JSON
/// `RedactOptions` (`{ "strict": bool, "allow": string[] }`). Returns a JSON
/// `RedactionResult`.
#[wasm_bindgen]
pub fn redact(text: &str, options_json: &str) -> Result<String, JsError> {
    let opts: tolkin_core::redact::RedactOptions = if options_json.trim().is_empty() {
        tolkin_core::redact::RedactOptions::default()
    } else {
        serde_json::from_str(options_json).map_err(|e| JsError::new(&e.to_string()))?
    };
    let result = tolkin_core::redact::redact(text, &opts);
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Estimate cost. `request_json` is a JSON `CostRequest` (only `model_id` and
/// `input_tokens` are required). Returns a JSON `CostBreakdown`.
#[wasm_bindgen]
pub fn cost(request_json: &str) -> Result<String, JsError> {
    let req: tolkin_core::cost::CostRequest =
        serde_json::from_str(request_json).map_err(|e| JsError::new(&e.to_string()))?;
    let breakdown = tolkin_core::cost::estimate(&req).map_err(|e| JsError::new(&e))?;
    serde_json::to_string(&breakdown).map_err(|e| JsError::new(&e.to_string()))
}

/// Every pricing model as a JSON array of `ModelPrice`.
#[wasm_bindgen]
pub fn models() -> Result<String, JsError> {
    serde_json::to_string(tolkin_core::pricing::all()).map_err(|e| JsError::new(&e.to_string()))
}

/// When the vendored pricing tables were last checked (YYYY-MM). A separate
/// function rather than a wrapper around `models()` so the existing web client,
/// which parses `models()` as a bare array, keeps working unchanged.
#[wasm_bindgen]
pub fn prices_observed() -> String {
    tolkin_core::pricing::PRICES_OBSERVED.to_string()
}

/// Audit `text` for token waste. `options_json` may be empty (or `{}`) for
/// defaults, or a JSON `AuditOptions`
/// (`{ "input_tokens": number | null, "include_experimental": bool }`).
/// Returns a JSON `AuditReport`.
#[wasm_bindgen]
pub fn audit(text: &str, options_json: &str) -> Result<String, JsError> {
    let opts: tolkin_core::audit::AuditOptions = if options_json.trim().is_empty() {
        tolkin_core::audit::AuditOptions::default()
    } else {
        serde_json::from_str(options_json).map_err(|e| JsError::new(&e.to_string()))?
    };
    let report = tolkin_core::audit::audit(text, &opts);
    serde_json::to_string(&report).map_err(|e| JsError::new(&e.to_string()))
}

/// Analyze an MCP config (JSON or JSONC) for `provider` ("openai" | "anthropic"
/// | "gemini"; anything else defaults to anthropic). Returns a JSON `McpAnalysis`.
#[wasm_bindgen]
pub fn analyze_mcp(config_text: &str, provider: &str) -> Result<String, JsError> {
    let provider = match provider {
        "openai" => tolkin_core::Provider::OpenAi,
        "gemini" => tolkin_core::Provider::Gemini,
        _ => tolkin_core::Provider::Anthropic,
    };
    let analysis =
        tolkin_core::mcp::analyze(config_text, provider).map_err(|e| JsError::new(&e))?;
    serde_json::to_string(&analysis).map_err(|e| JsError::new(&e.to_string()))
}
