//! Opt-in hybrid verification against Anthropic's free count_tokens endpoint.
//!
//! This is the only network egress in the CLI. It runs only when the user
//! passes --verify with their own API key, and callers must redact the text
//! before it reaches this module (count.rs runs the core redactor first).

use anyhow::{anyhow, Context, Result};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages/count_tokens";

/// Ask the Anthropic API for the exact input-token count of `text`.
pub fn anthropic_count(text: &str, api_key: &str, model: &str) -> Result<u64> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": text }],
    });

    let resp = ureq::post(ENDPOINT)
        .set("x-api-key", api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(body);

    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => {
            return Err(anyhow!("invalid API key (api.anthropic.com returned 401)"))
        }
        Err(ureq::Error::Status(429, _)) => {
            return Err(anyhow!(
                "rate limited by api.anthropic.com, try again shortly (429)"
            ))
        }
        Err(ureq::Error::Status(code, r)) => {
            let detail = r.into_string().unwrap_or_default();
            return Err(anyhow!("count_tokens failed (HTTP {code}): {detail}"));
        }
        Err(e) => return Err(anyhow!("request to api.anthropic.com failed: {e}")),
    };

    let json: serde_json::Value = resp
        .into_json()
        .context("failed to parse count_tokens response")?;
    json.get("input_tokens")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("count_tokens response missing input_tokens"))
}
