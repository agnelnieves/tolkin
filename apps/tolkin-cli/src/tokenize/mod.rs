use std::str::FromStr;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use tiktoken_rs::{cl100k_base, o200k_base, CoreBPE};
use tokenizers::Tokenizer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    OpenAi,
    Anthropic,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::OpenAi, Provider::Anthropic, Provider::Gemini];

    /// Stable identifier used in JSON output and CLI flags.
    pub fn slug(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
        }
    }

    /// Whether the count is exact for this provider.
    /// Anthropic ships through a cl100k_base proxy with documented ~10% error.
    pub fn is_estimate(self) -> bool {
        matches!(self, Provider::Anthropic)
    }
}

impl FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "openai" | "gpt" | "oai" => Ok(Provider::OpenAi),
            "anthropic" | "claude" => Ok(Provider::Anthropic),
            "gemini" | "google" | "gemma" => Ok(Provider::Gemini),
            other => Err(anyhow!(
                "unknown provider: {other} (expected openai, anthropic, or gemini)"
            )),
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::Gemini => "Gemini",
        };
        f.write_str(name)
    }
}

pub fn count(provider: Provider, text: &str) -> Result<usize> {
    if text.is_empty() {
        return Ok(0);
    }
    match provider {
        Provider::OpenAi => count_openai(text),
        Provider::Anthropic => count_anthropic(text),
        Provider::Gemini => count_gemini(text),
    }
}

/// One token's reconstructed text plus its byte span in the original input.
/// `start`/`end` are byte offsets so a caller can slice the source exactly.
pub struct Segment {
    pub id: u32,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Glyph shown when a token's bytes are a partial UTF-8 sequence that cannot
/// decode on its own (common with multibyte runes split across BPE tokens).
const REPLACEMENT_GLYPH: char = '\u{25AF}';

/// Per-token boundaries for the exact tokenizers (OpenAI, Gemini). Anthropic
/// returns an empty vec on purpose: cl100k_base is only a proxy for Claude, so
/// fabricating per-token splits would mislead. Use `count` for Anthropic.
pub fn segments(provider: Provider, text: &str) -> Result<Vec<Segment>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    match provider {
        Provider::OpenAi => segments_openai(text),
        Provider::Gemini => segments_gemini(text),
        Provider::Anthropic => Ok(Vec::new()),
    }
}

/// o200k_base is the BPE used by the GPT-4o and GPT-5 families.
fn openai_encoder() -> Result<&'static CoreBPE> {
    static ENCODER: OnceLock<CoreBPE> = OnceLock::new();
    if let Some(e) = ENCODER.get() {
        return Ok(e);
    }
    let bpe = o200k_base().context("failed to load o200k_base encoder")?;
    Ok(ENCODER.get_or_init(|| bpe))
}

/// cl100k_base is the BPE used by GPT-4 / GPT-3.5-turbo. It is the documented
/// public proxy for Claude until Anthropic publishes a real tokenizer; expect
/// roughly +/- 10% error compared to the count_tokens API.
fn anthropic_encoder() -> Result<&'static CoreBPE> {
    static ENCODER: OnceLock<CoreBPE> = OnceLock::new();
    if let Some(e) = ENCODER.get() {
        return Ok(e);
    }
    let bpe = cl100k_base().context("failed to load cl100k_base encoder")?;
    Ok(ENCODER.get_or_init(|| bpe))
}

const GEMMA_TOKENIZER_JSON: &[u8] = include_bytes!("../../assets/gemma-tokenizer.json");

fn gemini_tokenizer() -> Result<&'static Tokenizer> {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    if let Some(t) = TOKENIZER.get() {
        return Ok(t);
    }
    let tok = Tokenizer::from_bytes(GEMMA_TOKENIZER_JSON)
        .map_err(|e| anyhow!("failed to parse bundled Gemma tokenizer.json: {e}"))?;
    Ok(TOKENIZER.get_or_init(|| tok))
}

fn count_openai(text: &str) -> Result<usize> {
    let enc = openai_encoder()?;
    Ok(enc.encode_with_special_tokens(text).len())
}

fn count_anthropic(text: &str) -> Result<usize> {
    let enc = anthropic_encoder()?;
    Ok(enc.encode_with_special_tokens(text).len())
}

fn count_gemini(text: &str) -> Result<usize> {
    let tok = gemini_tokenizer()?;
    let encoding = tok
        .encode(text, false)
        .map_err(|e| anyhow!("gemma tokenization failed: {e}"))?;
    Ok(encoding.len())
}

/// CoreBPE exposes ids but not offsets, so decode each token's bytes on its own
/// and walk a running byte cursor. The concatenated token bytes reconstruct the
/// input exactly (asserted in tests), which keeps `start`/`end` accurate even
/// when a multibyte rune is split across tokens.
fn segments_openai(text: &str) -> Result<Vec<Segment>> {
    let enc = openai_encoder()?;
    let ids = enc.encode_with_special_tokens(text);
    let mut out = Vec::with_capacity(ids.len());
    let mut cursor = 0usize;
    for id in ids {
        let bytes = enc
            .decode_bytes(&[id])
            .map_err(|e| anyhow!("failed to decode token {id}: {e}"))?;
        let start = cursor;
        let end = cursor + bytes.len();
        let display = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => REPLACEMENT_GLYPH.to_string(),
        };
        out.push(Segment {
            id,
            text: display,
            start,
            end,
        });
        cursor = end;
    }
    Ok(out)
}

/// The HF tokenizer reports byte offsets directly, so use them for accuracy.
/// Gemma marks word boundaries with U+2581; convert it to a space for display
/// while keeping `start`/`end` anchored to the original input.
fn segments_gemini(text: &str) -> Result<Vec<Segment>> {
    let tok = gemini_tokenizer()?;
    let encoding = tok
        .encode(text, false)
        .map_err(|e| anyhow!("gemma tokenization failed: {e}"))?;
    let ids = encoding.get_ids();
    let tokens = encoding.get_tokens();
    let offsets = encoding.get_offsets();
    let mut out = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        let (start, end) = offsets[i];
        out.push(Segment {
            id: ids[i],
            text: token.replace('\u{2581}', " "),
            start,
            end,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_round_trip() {
        for p in Provider::ALL {
            let parsed: Provider = p.slug().parse().unwrap();
            assert_eq!(parsed, p);
        }
    }

    #[test]
    fn provider_aliases() {
        assert_eq!("gpt".parse::<Provider>().unwrap(), Provider::OpenAi);
        assert_eq!("claude".parse::<Provider>().unwrap(), Provider::Anthropic);
        assert_eq!("google".parse::<Provider>().unwrap(), Provider::Gemini);
    }

    #[test]
    fn openai_hello_world() {
        let n = count(Provider::OpenAi, "hello world").unwrap();
        assert!((2..=4).contains(&n), "got {n}");
    }

    #[test]
    fn anthropic_proxy_works() {
        let n = count(Provider::Anthropic, "hello world").unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn gemini_hello_world() {
        let n = count(Provider::Gemini, "hello world").unwrap();
        assert!((2..=5).contains(&n), "got {n}");
    }

    #[test]
    fn empty_string_is_zero() {
        for p in Provider::ALL {
            assert_eq!(count(p, "").unwrap(), 0, "{p:?}");
        }
    }

    #[test]
    fn segment_count_matches_token_count() {
        let text = "hello world, this is a token test 🚀 with unicode café";
        for p in [Provider::OpenAi, Provider::Gemini] {
            let segs = segments(p, text).unwrap();
            let n = count(p, text).unwrap();
            assert_eq!(segs.len(), n, "{p:?}");
        }
    }

    #[test]
    fn openai_segments_reconstruct_input() {
        // The raw token bytes (not the display strings, which lossily replace
        // partial-UTF-8 tokens) must concatenate back to the original input.
        let text = "hello world 🚀 café\nsecond line";
        let enc = openai_encoder().unwrap();
        let ids = enc.encode_with_special_tokens(text);
        let mut rebuilt = Vec::new();
        for id in &ids {
            rebuilt.extend_from_slice(&enc.decode_bytes(&[*id]).unwrap());
        }
        assert_eq!(rebuilt, text.as_bytes());

        // Spans must be contiguous and cover the whole input.
        let segs = segments(Provider::OpenAi, text).unwrap();
        assert_eq!(segs.first().unwrap().start, 0);
        assert_eq!(segs.last().unwrap().end, text.len());
        for pair in segs.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn gemini_offsets_anchor_to_input() {
        let text = "hello world";
        let segs = segments(Provider::Gemini, text).unwrap();
        for seg in &segs {
            assert!(seg.end <= text.len());
            assert!(seg.start <= seg.end);
        }
    }

    #[test]
    fn anthropic_segments_are_empty() {
        let segs = segments(Provider::Anthropic, "hello world").unwrap();
        assert!(segs.is_empty());
    }
}
