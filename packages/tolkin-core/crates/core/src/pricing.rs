//! Vendored pricing tables (mid-2026). All money fields are USD per 1,000,000
//! tokens. This is the single source of truth for the cost calculator on every
//! surface; refresh quarterly. See PLAN.md section 10.

use serde::Serialize;

use crate::Provider;

/// When the vendored rates below were last checked against the provider
/// pricing pages (YYYY-MM). Surfaced on every cost output so a stale table is
/// visible instead of silently wrong. Bump on every quarterly refresh.
pub const PRICES_OBSERVED: &str = "2026-06";

/// A higher pricing tier that kicks in past a context-length threshold. Gemini
/// 2.5 Pro roughly doubles its rates above 200K tokens (the "long-context cliff").
#[derive(Clone, Copy, Debug, Serialize)]
pub struct LongContext {
    pub threshold: u64,
    pub input: f64,
    pub output: f64,
}

/// One model's published rates. All money fields are USD per 1M tokens.
/// `cache_read` / `cache_write_*` are `None` when the vendor publishes no
/// distinct rate (the calculator then falls back to the base input rate).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ModelPrice {
    pub id: &'static str,
    pub provider: Provider,
    pub display: &'static str,
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write_5m: Option<f64>,
    pub cache_write_1h: Option<f64>,
    pub long_context: Option<LongContext>,
    /// The sensible default model for its provider (used by `compare` / cross
    /// provider views when the user has not picked a specific model).
    pub is_default: bool,
}

static MODELS: &[ModelPrice] = &[
    // Anthropic. Cache writes are the standard 1.25x (5m) / 2x (1h) of input.
    ModelPrice {
        id: "claude-opus-4.8",
        provider: Provider::Anthropic,
        display: "Claude Opus 4.8",
        input: 5.0,
        output: 25.0,
        cache_read: Some(0.50),
        cache_write_5m: Some(6.25),
        cache_write_1h: Some(10.0),
        long_context: None,
        is_default: false,
    },
    ModelPrice {
        id: "claude-opus-4.7",
        provider: Provider::Anthropic,
        display: "Claude Opus 4.7",
        input: 5.0,
        output: 25.0,
        cache_read: Some(0.50),
        cache_write_5m: Some(6.25),
        cache_write_1h: Some(10.0),
        long_context: None,
        is_default: false,
    },
    ModelPrice {
        id: "claude-sonnet-4.6",
        provider: Provider::Anthropic,
        display: "Claude Sonnet 4.6",
        input: 3.0,
        output: 15.0,
        cache_read: Some(0.30),
        cache_write_5m: Some(3.75),
        cache_write_1h: Some(6.0),
        long_context: None,
        is_default: true,
    },
    ModelPrice {
        id: "claude-haiku-4.5",
        provider: Provider::Anthropic,
        display: "Claude Haiku 4.5",
        input: 1.0,
        output: 5.0,
        cache_read: Some(0.10),
        cache_write_5m: Some(1.25),
        cache_write_1h: Some(2.0),
        long_context: None,
        is_default: false,
    },
    // OpenAI. Cached input is ~10% of base; no separate cache-write surcharge.
    ModelPrice {
        id: "gpt-5.5",
        provider: Provider::OpenAi,
        display: "GPT-5.5",
        input: 5.0,
        output: 30.0,
        cache_read: Some(0.50),
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: false,
    },
    ModelPrice {
        id: "gpt-5.4",
        provider: Provider::OpenAi,
        display: "GPT-5.4",
        input: 2.5,
        output: 15.0,
        cache_read: Some(0.25),
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: true,
    },
    ModelPrice {
        id: "gpt-5.4-mini",
        provider: Provider::OpenAi,
        display: "GPT-5.4 mini",
        input: 0.75,
        output: 4.5,
        cache_read: Some(0.075),
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: false,
    },
    ModelPrice {
        id: "gpt-5.4-nano",
        provider: Provider::OpenAi,
        display: "GPT-5.4 nano",
        input: 0.20,
        output: 1.25,
        cache_read: Some(0.02),
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: false,
    },
    // Gemini. 2.5 Pro has the 200K long-context cliff; no published cache-read
    // discount is modeled here.
    ModelPrice {
        id: "gemini-2.5-pro",
        provider: Provider::Gemini,
        display: "Gemini 2.5 Pro",
        input: 1.25,
        output: 10.0,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: Some(LongContext {
            threshold: 200_000,
            input: 2.5,
            output: 15.0,
        }),
        is_default: true,
    },
    ModelPrice {
        id: "gemini-2.5-flash",
        provider: Provider::Gemini,
        display: "Gemini 2.5 Flash",
        input: 0.30,
        output: 2.5,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: false,
    },
    ModelPrice {
        id: "gemini-2.5-flash-lite",
        provider: Provider::Gemini,
        display: "Gemini 2.5 Flash-Lite",
        input: 0.10,
        output: 0.40,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        long_context: None,
        is_default: false,
    },
];

/// Every model in the catalog.
pub fn all() -> &'static [ModelPrice] {
    MODELS
}

/// Look up a model by its stable id.
pub fn find(id: &str) -> Option<&'static ModelPrice> {
    MODELS.iter().find(|m| m.id == id)
}

/// The default model for a provider (first `is_default`, else first listed).
pub fn default_for(provider: Provider) -> &'static ModelPrice {
    MODELS
        .iter()
        .find(|m| m.provider == provider && m.is_default)
        .or_else(|| MODELS.iter().find(|m| m.provider == provider))
        .expect("every provider has at least one model")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_known_and_unknown() {
        assert!(find("claude-sonnet-4.6").is_some());
        assert!(find("does-not-exist").is_none());
    }

    #[test]
    fn exactly_one_default_per_provider() {
        for p in [Provider::OpenAi, Provider::Anthropic, Provider::Gemini] {
            let n = all()
                .iter()
                .filter(|m| m.provider == p && m.is_default)
                .count();
            assert_eq!(n, 1, "{p:?}");
            assert_eq!(default_for(p).provider, p);
        }
    }

    #[test]
    fn output_never_cheaper_than_input() {
        for m in all() {
            assert!(m.output >= m.input, "{}", m.id);
        }
    }
}
