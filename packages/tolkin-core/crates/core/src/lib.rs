//! Tolkin core: the single source of truth for pricing, cost estimation, and
//! secret redaction shared by the CLI (native rlib) and the web app (WASM).
//!
//! Tokenization is intentionally NOT here. It stays platform-native (JS libs in
//! the browser, Rust crates in the CLI) because reinventing BPE/SentencePiece
//! for cross-platform parity is poor ROI. The core treats token counts as a
//! primitive input.

pub mod audit;
pub mod cost;
pub mod format;
pub mod mcp;
pub mod mcp_tools;
pub mod pricing;
pub mod redact;

use serde::{Deserialize, Serialize};

/// The three frontier providers Tolkin supports in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAi,
    Anthropic,
    Gemini,
}

impl Provider {
    /// Stable lowercase identifier used in JSON output and CLI flags.
    pub fn slug(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
        }
    }

    /// Human-facing provider name.
    pub fn display(self) -> &'static str {
        match self {
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::Gemini => "Gemini",
        }
    }
}

/// Crate version, surfaced through the WASM bindings to prove the wiring.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn provider_slug_round_trips_through_serde() {
        for p in [Provider::OpenAi, Provider::Anthropic, Provider::Gemini] {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.slug()));
            let back: Provider = serde_json::from_str(&json).unwrap();
            assert_eq!(back, p);
        }
    }
}
