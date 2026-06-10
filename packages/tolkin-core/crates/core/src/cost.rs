//! Cost calculator. Pure arithmetic over the `pricing` tables and a token count.
//!
//! Input-token-bounded by design: output tokens are zero by default, and the
//! per-call total reports input-side cost only unless the caller either supplies
//! `output_tokens` or sets `estimate_output: true` to opt into the rough
//! volume assumption derived from the provider's output:input PRICE ratio.
//! Shared by the CLI and the web (via WASM). See PLAN.md section 10.

use serde::{Deserialize, Serialize};

use crate::pricing::{self, ModelPrice};
use crate::Provider;

/// Anthropic prompt-cache TTL. Picks which cache-write rate applies.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CacheTtl {
    #[default]
    #[serde(rename = "5m")]
    FiveMin,
    #[serde(rename = "1h")]
    OneHour,
}

fn default_calls() -> u64 {
    1
}

/// A cost-estimate request. Only `model_id` and `input_tokens` are required;
/// every other field has a sensible default so JSON callers can omit them.
#[derive(Clone, Debug, Deserialize)]
pub struct CostRequest {
    pub model_id: String,
    pub input_tokens: u64,
    /// Output tokens. `None` plus `estimate_output: false` bills zero output;
    /// `None` plus `estimate_output: true` reproduces the legacy rough volume
    /// assumption (input times the provider output:input price ratio).
    #[serde(default)]
    pub output_tokens: Option<u64>,
    /// Opt in to the volume-from-price-ratio output estimate when
    /// `output_tokens` is None. Defaults to false so the headline total stays
    /// input-side and matches the product's input-token-bounded identity.
    #[serde(default)]
    pub estimate_output: bool,
    /// How many calls to multiply the per-call cost by (monthly volume, etc).
    #[serde(default = "default_calls")]
    pub calls: u64,
    /// Fraction of input served from cache (0..1).
    #[serde(default)]
    pub cache_hit_rate: f64,
    /// Tokens written to cache on a cold turn (billed at the cache-write rate).
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub cache_ttl: CacheTtl,
    /// Fraction of input+output eligible for the Batch API 50% discount (0..1).
    #[serde(default)]
    pub batch_fraction: f64,
    /// Force the higher long-context tier even below the threshold.
    #[serde(default)]
    pub long_context: bool,
}

impl CostRequest {
    /// Minimal request: a model and an input token count, everything else default.
    pub fn new(model_id: impl Into<String>, input_tokens: u64) -> Self {
        Self {
            model_id: model_id.into(),
            input_tokens,
            output_tokens: None,
            estimate_output: false,
            calls: 1,
            cache_hit_rate: 0.0,
            cache_write_tokens: 0,
            cache_ttl: CacheTtl::FiveMin,
            batch_fraction: 0.0,
            long_context: false,
        }
    }
}

/// Dollar amounts for one slice of work (per call or per total).
#[derive(Clone, Debug, Serialize)]
pub struct CallCost {
    pub fresh_input: f64,
    pub cached_input: f64,
    pub cache_write: f64,
    pub output: f64,
    pub total: f64,
}

/// Full cost breakdown with the token math that produced it and honesty notes.
#[derive(Clone, Debug, Serialize)]
pub struct CostBreakdown {
    pub model_id: String,
    pub provider: Provider,
    pub display: String,
    pub input_tokens: u64,
    pub fresh_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub output_estimated: bool,
    pub long_context_applied: bool,
    pub calls: u64,
    pub per_call: CallCost,
    pub total: CallCost,
    pub notes: Vec<String>,
}

/// Estimate by model id. Errors if the id is not in the catalog.
pub fn estimate(req: &CostRequest) -> Result<CostBreakdown, String> {
    let model =
        pricing::find(&req.model_id).ok_or_else(|| format!("unknown model: {}", req.model_id))?;
    Ok(estimate_with(model, req))
}

/// Estimate against a specific model.
pub fn estimate_with(model: &ModelPrice, req: &CostRequest) -> CostBreakdown {
    let hit = req.cache_hit_rate.clamp(0.0, 1.0);
    let batch = req.batch_fraction.clamp(0.0, 1.0);
    let input = req.input_tokens;
    let calls = req.calls.max(1);
    let mut notes = Vec::new();

    // Long-context cliff (Gemini): swap to the higher tier above the threshold.
    let (mut in_rate, mut out_rate) = (model.input, model.output);
    let mut long_context_applied = false;
    if let Some(lc) = model.long_context {
        if req.long_context || input > lc.threshold {
            in_rate = lc.input;
            out_rate = lc.output;
            long_context_applied = true;
            notes.push(format!(
                "Long-context pricing: inputs over {} tokens bill at the higher tier (${:.2}/${:.2} per 1M in/out).",
                lc.threshold, lc.input, lc.output
            ));
        }
    }

    let ratio = model.output / model.input;
    let (output_tokens, output_estimated) = match req.output_tokens {
        Some(o) => (o, false),
        None if req.estimate_output => ((input as f64 * ratio).round() as u64, true),
        None => (0, false),
    };

    let cached = (input as f64 * hit).round() as u64;
    let fresh = input.saturating_sub(cached);

    let per_million = |tokens: u64, rate: f64| (tokens as f64) / 1_000_000.0 * rate;

    let cache_read_rate = match model.cache_read {
        Some(r) => r,
        None => {
            if hit > 0.0 {
                notes.push(
                    "No published cache-read discount for this model; cached input is billed at the full input rate."
                        .to_string(),
                );
            }
            in_rate
        }
    };

    let mut fresh_cost = per_million(fresh, in_rate);
    let cached_cost = per_million(cached, cache_read_rate);
    let mut output_cost = per_million(output_tokens, out_rate);

    let cache_write_rate = match req.cache_ttl {
        CacheTtl::FiveMin => model.cache_write_5m,
        CacheTtl::OneHour => model.cache_write_1h,
    };
    let cache_write_cost = match cache_write_rate {
        Some(r) if req.cache_write_tokens > 0 => per_million(req.cache_write_tokens, r),
        _ => 0.0,
    };

    if batch > 0.0 {
        let keep = 1.0 - 0.5 * batch;
        fresh_cost *= keep;
        output_cost *= keep;
        notes.push(format!(
            "Batch API: {:.0}% of input+output billed at 50% off (24h SLA).",
            batch * 100.0
        ));
    }

    let per_total = fresh_cost + cached_cost + output_cost + cache_write_cost;

    if req.output_tokens.is_none() {
        if req.estimate_output {
            notes.push(format!(
                "Output volume is a rough assumption: input tokens times the {:.0}x output:input PRICE ratio for {}. Real responses vary widely; supply an output token count to model your workload.",
                ratio, model.display
            ));
        } else {
            notes
                .push("Output not included; supply an output token count to model it.".to_string());
        }
    }
    notes.push("Input-token-bounded. Output cost varies by response.".to_string());
    notes.push(format!(
        "Prices observed {}; verify against provider pricing pages before relying on dollar figures.",
        pricing::PRICES_OBSERVED
    ));
    match model.provider {
        Provider::OpenAi => notes.push(
            "Reasoning tokens (o-series / GPT thinking) bill at the output rate and are not returned in the response body."
                .to_string(),
        ),
        Provider::Anthropic => notes.push(
            "Extended thinking bills at the output rate even when the thinking text is omitted from the response."
                .to_string(),
        ),
        Provider::Gemini => {}
    }

    let scale = |c: f64| c * calls as f64;
    CostBreakdown {
        model_id: model.id.to_string(),
        provider: model.provider,
        display: model.display.to_string(),
        input_tokens: input,
        fresh_tokens: fresh,
        cached_tokens: cached,
        output_tokens,
        output_estimated,
        long_context_applied,
        calls,
        per_call: CallCost {
            fresh_input: fresh_cost,
            cached_input: cached_cost,
            cache_write: cache_write_cost,
            output: output_cost,
            total: per_total,
        },
        total: CallCost {
            fresh_input: scale(fresh_cost),
            cached_input: scale(cached_cost),
            cache_write: scale(cache_write_cost),
            output: scale(output_cost),
            total: scale(per_total),
        },
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn sonnet_basic_no_cache() {
        // Default path: output_tokens=None and estimate_output=false means the
        // headline total is input-side only. The old behaviour (output billed
        // as input times the output:input price ratio) is now opt-in.
        let b = estimate(&CostRequest::new("claude-sonnet-4.6", 1000)).unwrap();
        assert_eq!(b.output_tokens, 0);
        assert!(!b.output_estimated);
        approx(b.per_call.fresh_input, 0.003);
        approx(b.per_call.output, 0.0);
        approx(b.per_call.total, 0.003);
        assert!(b.notes.iter().any(|n| n.contains("Output not included")));
    }

    #[test]
    fn default_output_is_zero_and_not_estimated() {
        // Same shape as sonnet_basic_no_cache but on gpt-5.4 to prove this is
        // not provider-specific. Asserts both the zero-cost outcome and the
        // "output not included" honesty note.
        let b = estimate(&CostRequest::new("gpt-5.4", 1001)).unwrap();
        assert_eq!(b.output_tokens, 0);
        assert!(!b.output_estimated);
        approx(b.per_call.output, 0.0);
        // Input-side cost is the whole per-call total: 1001 / 1e6 * $2.50.
        approx(b.per_call.fresh_input, 1001.0 / 1_000_000.0 * 2.5);
        approx(b.per_call.total, 1001.0 / 1_000_000.0 * 2.5);
        assert!(b.notes.iter().any(|n| n.contains("Output not included")));
    }

    #[test]
    fn opt_in_output_estimate_uses_price_ratio() {
        // Independent confirmation of the money math: the opt-in estimate
        // must reproduce input * (output_rate / input_rate) at the price
        // table's actual numbers, not a hardcoded constant. This guards
        // against silent drift if pricing changes.
        let model = pricing::find("claude-sonnet-4.6").unwrap();
        let mut req = CostRequest::new("claude-sonnet-4.6", 1000);
        req.estimate_output = true;
        let b = estimate(&req).unwrap();

        let expected_output_tokens = (1000.0 * (model.output / model.input)).round() as u64;
        let expected_output_cost = expected_output_tokens as f64 / 1_000_000.0 * model.output;
        let expected_input_cost = 1000.0 / 1_000_000.0 * model.input;

        assert_eq!(b.output_tokens, expected_output_tokens);
        assert!(b.output_estimated);
        approx(b.per_call.output, expected_output_cost);
        approx(b.per_call.fresh_input, expected_input_cost);
        approx(b.per_call.total, expected_input_cost + expected_output_cost);
        // The disclosure note must label the basis (price ratio + rough volume
        // assumption) so a reader knows the figure is not measurement.
        assert!(b
            .notes
            .iter()
            .any(|n| n.contains("rough assumption") && n.contains("PRICE ratio")));
    }

    #[test]
    fn full_cache_hit_uses_cache_read_rate() {
        let mut req = CostRequest::new("claude-sonnet-4.6", 1000);
        req.cache_hit_rate = 1.0;
        req.output_tokens = Some(0);
        let b = estimate(&req).unwrap();
        assert_eq!(b.cached_tokens, 1000);
        assert_eq!(b.fresh_tokens, 0);
        approx(b.per_call.cached_input, 0.0003); // 1000/1e6 * 0.30
        approx(b.per_call.fresh_input, 0.0);
    }

    #[test]
    fn calls_multiply_total() {
        let mut req = CostRequest::new("gpt-5.4", 1000);
        req.calls = 10;
        let b = estimate(&req).unwrap();
        approx(b.total.total, b.per_call.total * 10.0);
    }

    #[test]
    fn gemini_long_context_cliff() {
        let mut req = CostRequest::new("gemini-2.5-pro", 250_000);
        req.output_tokens = Some(0);
        let b = estimate(&req).unwrap();
        assert!(b.long_context_applied);
        approx(b.per_call.fresh_input, 250_000.0 / 1_000_000.0 * 2.5);
    }

    #[test]
    fn batch_halves_eligible_fraction() {
        let mut req = CostRequest::new("gpt-5.4", 1_000_000);
        req.output_tokens = Some(0);
        req.batch_fraction = 1.0;
        let b = estimate(&req).unwrap();
        approx(b.per_call.fresh_input, 2.5 * 0.5);
    }

    #[test]
    fn notes_carry_prices_observed_marker() {
        let b = estimate(&CostRequest::new("gpt-5.4", 1000)).unwrap();
        assert!(b
            .notes
            .iter()
            .any(|n| n.contains(crate::pricing::PRICES_OBSERVED)));
    }

    #[test]
    fn unknown_model_errors() {
        assert!(estimate(&CostRequest::new("nope", 10)).is_err());
    }

    #[test]
    fn deserializes_minimal_json() {
        let req: CostRequest =
            serde_json::from_str(r#"{"model_id":"gpt-5.4","input_tokens":100}"#).unwrap();
        assert_eq!(req.calls, 1);
        assert_eq!(req.cache_ttl, CacheTtl::FiveMin);
        // Backward-compatible JSON contract: callers that omit estimate_output
        // get the new input-side default for free.
        assert!(!req.estimate_output);
        assert!(estimate(&req).is_ok());
    }

    #[test]
    fn json_can_opt_in_to_output_estimate() {
        // Web (WASM) callers route through this exact JSON shape, so the
        // field name has to land verbatim.
        let req: CostRequest = serde_json::from_str(
            r#"{"model_id":"gpt-5.4","input_tokens":1000,"estimate_output":true}"#,
        )
        .unwrap();
        assert!(req.estimate_output);
        let b = estimate(&req).unwrap();
        assert!(b.output_estimated);
        assert!(b.output_tokens > 0);
    }
}
