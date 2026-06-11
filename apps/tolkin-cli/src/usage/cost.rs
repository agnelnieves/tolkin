//! Model id normalization and per-session cost arithmetic.
//!
//! Raw log ids drift in two predictable ways: providers report dotted versions
//! as hyphenated (claude-opus-4-7) and Anthropic publishes dated aliases
//! (claude-haiku-4-5-20251001). The catalog in `tolkin-core::pricing` keys on
//! the dotted, undated id. We normalize before lookup and return None for ids
//! we cannot recognize, so unpriced traffic stays visible instead of silently
//! being charged at a guessed rate.

use tolkin_core::pricing;

use super::types::UsageTotals;

/// Lowercase, trim, strip a trailing `-YYYYMMDD` date alias, and rewrite
/// `-N-M` minor-version hyphens as `-N.M`.
pub fn normalize_model_id(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    s = strip_trailing_date(&s);
    s = collapse_minor_version_hyphens(&s);
    s
}

fn strip_trailing_date(s: &str) -> String {
    // Match `-YYYYMMDD` at the end, where Y/M/D are all digits and the segment
    // is exactly 8 chars long. We do not try harder than that because Anthropic
    // only publishes the eight-digit form.
    if let Some(idx) = s.rfind('-') {
        let tail = &s[idx + 1..];
        if tail.len() == 8 && tail.chars().all(|c| c.is_ascii_digit()) {
            return s[..idx].to_string();
        }
    }
    s.to_string()
}

fn collapse_minor_version_hyphens(s: &str) -> String {
    // Walk segments, joining `<digits>-<digits>` adjacent pairs with a dot.
    // Multiple pairs in a row (e.g. claude-foo-4-7-2) collapse left-to-right.
    let segments: Vec<&str> = s.split('-').collect();
    if segments.len() < 2 {
        return s.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(segments.len());
    for seg in segments {
        if let Some(last) = out.last() {
            let last_is_digits = !last.is_empty() && last.chars().all(|c| c.is_ascii_digit());
            let seg_is_digits = !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit());
            if last_is_digits && seg_is_digits {
                let combined = format!("{last}.{seg}");
                out.pop();
                out.push(combined);
                continue;
            }
        }
        out.push(seg.to_string());
    }
    out.join("-")
}

/// Per-million-token unit conversion: `tokens * usd_per_million / 1_000_000`.
fn cost_of(tokens: u64, rate_usd_per_million: f64) -> f64 {
    (tokens as f64) * rate_usd_per_million / 1_000_000.0
}

/// Base input rate in USD per million tokens for the given raw model id.
/// Returns None for unknown or unrecognized ids. Used by the model-mix
/// advisory to identify the cheapest-tier model by its input price.
pub fn input_rate_usd_per_mtok(raw_model: &str) -> Option<f64> {
    let id = normalize_model_id(raw_model);
    pricing::find(&id).map(|m| m.input)
}

/// Input-side rates for the cache analysis, resolved with exactly the same
/// normalization and missing-rate fallbacks as [`cost_usd`] so the two
/// surfaces can never disagree on what a model's cache tokens cost. Returns
/// None for unknown ids (the analysis then reports the model as unpriced).
pub fn cache_rates(raw_model: &str) -> Option<crate::cache_analysis::ModelRates> {
    let id = normalize_model_id(raw_model);
    let m = pricing::find(&id)?;
    Some(crate::cache_analysis::ModelRates {
        cache_read: m.cache_read.unwrap_or(m.input),
        cache_write_5m: m.cache_write_5m.unwrap_or(m.input),
        cache_write_1h: m.cache_write_1h.unwrap_or(m.input),
    })
}

/// USD spend for a totals block at one model's rates. Returns None for unknown
/// or unpriced ids. When the catalog has no `cache_read` / `cache_write_*`
/// rate, we fall back to the base input rate for those tokens (the calculator
/// already does this for the surface; we replicate the convention here so
/// per-session cost matches per-message cost).
pub fn cost_usd(raw_model: &str, totals: &UsageTotals) -> Option<f64> {
    let id = normalize_model_id(raw_model);
    let m = pricing::find(&id)?;
    let cache_read_rate = m.cache_read.unwrap_or(m.input);
    let cache_5m_rate = m.cache_write_5m.unwrap_or(m.input);
    let cache_1h_rate = m.cache_write_1h.unwrap_or(m.input);
    let total = cost_of(totals.input_tokens, m.input)
        + cost_of(totals.cache_read_tokens, cache_read_rate)
        + cost_of(totals.cache_write_5m_tokens, cache_5m_rate)
        + cost_of(totals.cache_write_1h_tokens, cache_1h_rate)
        + cost_of(totals.output_tokens, m.output);
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_hyphenated_minor_version() {
        assert_eq!(normalize_model_id("claude-opus-4-7"), "claude-opus-4.7");
        assert_eq!(normalize_model_id("CLAUDE-OPUS-4-7"), "claude-opus-4.7");
        assert_eq!(normalize_model_id("  claude-opus-4-7  "), "claude-opus-4.7");
    }

    #[test]
    fn normalize_strips_trailing_date_alias() {
        assert_eq!(
            normalize_model_id("claude-haiku-4-5-20251001"),
            "claude-haiku-4.5"
        );
        // A six-digit suffix is NOT a date in the alias scheme; leave it as
        // a trailing hyphen segment (no dot collapse: "4.5" already contains a
        // dot so the digits-adjacency rule does not fire).
        assert_eq!(
            normalize_model_id("claude-haiku-4-5-202510"),
            "claude-haiku-4.5-202510"
        );
    }

    #[test]
    fn normalize_leaves_clean_ids_alone() {
        assert_eq!(normalize_model_id("claude-sonnet-4.6"), "claude-sonnet-4.6");
        assert_eq!(normalize_model_id("gpt-5.4"), "gpt-5.4");
        assert_eq!(normalize_model_id("gemini-2.5-pro"), "gemini-2.5-pro");
    }

    #[test]
    fn unpriceable_ids_return_none() {
        // Models that genuinely have no catalog entry on this machine. Tolkin
        // surfaces them as unpriced rather than guessing.
        let totals = UsageTotals {
            input_tokens: 100,
            output_tokens: 100,
            ..UsageTotals::default()
        };
        assert!(cost_usd("claude-fable-5", &totals).is_none());
        assert!(cost_usd("claude-opus-4-6", &totals).is_none());
        assert!(cost_usd("sonnet", &totals).is_none());
        assert!(cost_usd("", &totals).is_none());
    }

    #[test]
    fn cost_golden_against_pricing_constants() {
        // claude-sonnet-4.6 rates: input 3.0, output 15.0, cache_read 0.30,
        // cache_write_5m 3.75, cache_write_1h 6.0 USD per 1M tokens.
        // Hand-computed for these counts:
        //   1_000_000 input -> 3.0
        //   2_000_000 output -> 30.0
        //   500_000 cache_read -> 0.15
        //   400_000 cache_write_5m -> 1.50
        //   100_000 cache_write_1h -> 0.60
        // Sum = 35.25 USD.
        let totals = UsageTotals {
            input_tokens: 1_000_000,
            output_tokens: 2_000_000,
            cache_read_tokens: 500_000,
            cache_write_5m_tokens: 400_000,
            cache_write_1h_tokens: 100_000,
        };
        let usd = cost_usd("claude-sonnet-4-6", &totals).unwrap();
        assert!((usd - 35.25).abs() < 1e-9, "got {usd}");
    }

    #[test]
    fn cost_uses_published_gemini_cache_rate() {
        // Google publishes Gemini 2.5 Flash cache_read at 0.03 USD per 1M
        // tokens (10% of base input, verified 2026-06). 1M input at 0.30 +
        // 1M cache_read at 0.03 = 0.33 USD.
        let totals = UsageTotals {
            input_tokens: 1_000_000,
            cache_read_tokens: 1_000_000,
            output_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
        };
        let usd = cost_usd("gemini-2.5-flash", &totals).unwrap();
        let m = pricing::find("gemini-2.5-flash").unwrap();
        let expected = m.input + m.cache_read.unwrap();
        assert!(
            (usd - expected).abs() < 1e-9,
            "got {usd}, expected {expected}"
        );
        assert!((usd - 0.33).abs() < 1e-9, "got {usd}");
    }

    #[test]
    fn cost_falls_back_to_input_rate_when_cache_rate_missing() {
        // Every catalog model now publishes cache rates, so this test
        // documents the fallback contract directly: if a model had a None
        // cache_read in the table, those tokens would bill at the input
        // rate. We exercise that by patching a totals block such that the
        // contract is observable through unit math on the catalog: for any
        // model M with no cache_write_1h, 1M cache_write_1h tokens cost
        // exactly 1M * M.input (the cache_write_1h fallback). gpt-5.4 has
        // input 2.5 and cache_write_1h = None.
        let m = pricing::find("gpt-5.4").unwrap();
        assert!(m.cache_write_1h.is_none(), "fixture model invariant");
        let totals = UsageTotals {
            input_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 1_000_000,
        };
        let usd = cost_usd("gpt-5.4", &totals).unwrap();
        assert!(
            (usd - m.input).abs() < 1e-9,
            "got {usd}, expected {}",
            m.input
        );
    }
}
