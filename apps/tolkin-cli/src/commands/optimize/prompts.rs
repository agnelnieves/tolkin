//! Prompt builders and the latency estimate table for `tolkin optimize`.
//!
//! Prompt discipline (MLX-RESEARCH-FINDINGS.md section 3): schema in prompt,
//! "ONLY a JSON object", explicit max_tokens, low temperature. The skill
//! rubric is a static byte-identical prefix across every file so the
//! server's prompt cache absorbs the rubric prefill once (section 4.5).

use crate::tokenize::{self, Provider as TokProvider};

/// Narrate task generation budget.
pub const NARRATE_MAX_TOKENS: u32 = 600;
/// Skill lint per-file generation budget.
pub const SKILL_MAX_TOKENS: u32 = 700;
/// Both tasks decode at low temperature for stable, citable prose.
pub const TEMPERATURE: f32 = 0.2;
/// Skill file content is truncated past this many tokens.
pub const SKILL_INPUT_TOKEN_CAP: usize = 6000;
/// Marker appended where a skill file was cut.
pub const TRUNCATION_MARKER: &str = "\n[truncated by tolkin at ~6000 tokens]";

/// System prompt for the narration task. The model may only restate numbers
/// already present in the input JSON; the number-faithfulness gate enforces
/// that claim deterministically after the fact.
pub const NARRATE_SYSTEM: &str = "You are a local advisor running on the user's machine. You explain a deterministic token-usage summary of their AI agent context in plain language.\n\
Rules:\n\
- Use ONLY numbers present in the input JSON. Never invent counts, prices, or percentages.\n\
- Plain language a non-technical reader can follow, under 350 words.\n\
- Structure: first what is loaded, then why it costs, then the top 3 concrete actions, each naming an exact file path from the input.\n\
- Your output is advisory only; it never measures anything.";

/// User prompt for the narration task: the skeleton, nothing else.
pub fn narrate_user(skeleton_json: &str) -> String {
    format!("Input JSON:\n{skeleton_json}\n\nExplain this summary now.")
}

/// Retry system prompt after a number-faithfulness failure: the original
/// system plus the exact violations, so the model can self-correct once.
pub fn narrate_retry_system(violations: &[String]) -> String {
    format!(
        "{NARRATE_SYSTEM}\n\
         Your previous answer used numbers that are NOT in the input JSON: {}. \
         Remove or replace them; only restate numbers from the input JSON.",
        violations.join(", ")
    )
}

/// Static lint rubric, byte-identical across files (static prefix first so
/// the sidecar's prompt cache can reuse the rubric prefill per file).
pub const SKILL_SYSTEM: &str = "You are a local lint advisor for agent skill files (SKILL.md). Evaluate the file against this rubric:\n\
- trigger: is the description specific about when the skill should activate, or vague enough to misfire?\n\
- verbosity: is the body padded with filler the agent does not need?\n\
- examples: are examples bloated, redundant, or longer than the rule they illustrate?\n\
- structure: do headings and sections guide the agent, or bury the load-bearing instructions?\n\
Respond with ONLY a JSON object, no prose, matching exactly:\n\
{\"findings\":[{\"aspect\":\"trigger|verbosity|examples|structure\",\"severity\":\"high|medium|low\",\"evidence_quote\":\"verbatim short quote from the file\",\"suggestion\":\"concrete fix\"}],\"summary\":\"under 200 chars\"}\n\
Every evidence_quote must be copied verbatim from the file content.";

/// Retry system prompt after a truncated response: same static rubric prefix
/// plus a tighter ask.
pub fn skill_retry_system() -> String {
    format!("{SKILL_SYSTEM}\nReturn at most 2 findings.")
}

/// JSON schema for the skill lint payload. Enforced server-side only on
/// backends that honor response_format json_schema; always validated
/// locally regardless.
pub fn skill_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "aspect": { "type": "string", "enum": ["trigger", "verbosity", "examples", "structure"] },
                        "severity": { "type": "string", "enum": ["high", "medium", "low"] },
                        "evidence_quote": { "type": "string" },
                        "suggestion": { "type": "string" }
                    },
                    "required": ["aspect", "severity", "evidence_quote", "suggestion"]
                }
            },
            "summary": { "type": "string" }
        },
        "required": ["findings", "summary"]
    })
}

/// User prompt for one skill file: the redacted content, truncated past the
/// token cap with a visible marker. Returns the prompt and whether input
/// truncation happened. Content MUST already be redacted by the caller.
pub fn skill_user(redacted: &str) -> (String, bool) {
    let truncated = match tokenize::segments(TokProvider::OpenAi, redacted) {
        Ok(segs) if segs.len() > SKILL_INPUT_TOKEN_CAP => {
            let mut cut = segs[SKILL_INPUT_TOKEN_CAP - 1].end.min(redacted.len());
            while cut > 0 && !redacted.is_char_boundary(cut) {
                cut -= 1;
            }
            let mut s = redacted[..cut].to_string();
            s.push_str(TRUNCATION_MARKER);
            Some(s)
        }
        _ => None,
    };
    match truncated {
        Some(s) => (format!("SKILL.md content:\n{s}"), true),
        None => (format!("SKILL.md content:\n{redacted}"), false),
    }
}

/// Count tokens for an estimate. Tokenizer failures fall back to a
/// bytes/4 heuristic so the estimate degrades instead of erroring.
pub fn estimate_tokens(text: &str) -> usize {
    tokenize::count(TokProvider::OpenAi, text).unwrap_or(text.len() / 4)
}

/// Per-chip prefill and decode rates in tokens per second.
///
/// Basis: MLX-RESEARCH-FINDINGS.md section 2. The M1 Max row is measured on
/// real hardware (mlx-lm, Qwen3.5-4B-4bit, about 250 tok/s prefill plateau
/// and 40 tok/s decode mid-context). Every other row is an estimate scaled
/// from the verified llama.cpp pp512 ratios; the unknown fallback is
/// deliberately conservative.
#[derive(Clone, Debug)]
pub struct ChipRates {
    pub label: String,
    pub prefill: f64,
    pub decode: f64,
    pub measured: bool,
}

/// Static rate table. Longest names first per family so "M1 Max" wins over
/// "M1" during substring matching. Ultra-class and future chips fall through
/// to their base entry or to the conservative unknown row.
const RATES: &[(&str, f64, f64, bool)] = &[
    ("M1 Max", 250.0, 40.0, true),
    ("M1 Pro", 170.0, 20.0, false),
    ("M2 Max", 250.0, 35.0, false),
    ("M2 Pro", 170.0, 20.0, false),
    ("M3 Max", 300.0, 38.0, false),
    ("M3 Pro", 190.0, 22.0, false),
    ("M4 Max", 420.0, 35.0, false),
    ("M4 Pro", 210.0, 25.0, false),
    ("M5", 350.0, 45.0, false),
    ("M4", 105.0, 15.0, false),
    ("M3", 100.0, 14.0, false),
    ("M2", 85.0, 12.0, false),
    ("M1", 110.0, 12.0, false),
];

/// Conservative fallback when the chip is unknown or detection failed.
const UNKNOWN_RATES: (f64, f64) = (85.0, 12.0);

/// Map a CPU brand string to its rate row. None or unmatched brands get the
/// conservative unknown row.
pub fn chip_rates(brand: Option<&str>) -> ChipRates {
    if let Some(b) = brand {
        for (name, prefill, decode, measured) in RATES {
            if b.contains(name) {
                return ChipRates {
                    label: (*name).to_string(),
                    prefill: *prefill,
                    decode: *decode,
                    measured: *measured,
                };
            }
        }
    }
    ChipRates {
        label: "unknown chip (conservative)".to_string(),
        prefill: UNKNOWN_RATES.0,
        decode: UNKNOWN_RATES.1,
        measured: false,
    }
}

/// Detect the chip via `sysctl machdep.cpu.brand_string`. Failure of any
/// kind (non-macOS, sandbox, missing binary) is tolerated and yields None.
pub fn detect_chip() -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Seconds for one chat call: prompt_tokens / prefill + max_tokens / decode,
/// rounded up. Always an estimate, never a measurement.
pub fn eta_seconds(prompt_tokens: usize, max_tokens: u32, rates: &ChipRates) -> u64 {
    let secs = prompt_tokens as f64 / rates.prefill + f64::from(max_tokens) / rates.decode;
    secs.ceil() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_math_matches_the_formula() {
        let m1_max = chip_rates(Some("Apple M1 Max"));
        assert!(m1_max.measured);
        assert_eq!(m1_max.label, "M1 Max");
        // 5000/250 = 20s prefill, 600/40 = 15s decode, total 35s.
        assert_eq!(eta_seconds(5000, 600, &m1_max), 35);
        // Fractions round up.
        assert_eq!(eta_seconds(1, 1, &m1_max), 1);
    }

    #[test]
    fn chip_fallback_is_conservative_and_labeled() {
        let unknown = chip_rates(None);
        assert!(!unknown.measured);
        assert!(unknown.label.contains("conservative"));
        assert_eq!(unknown.prefill, 85.0);
        assert_eq!(unknown.decode, 12.0);

        let alien = chip_rates(Some("Intel(R) Core(TM) i9"));
        assert!(alien.label.contains("conservative"));
    }

    #[test]
    fn variant_names_win_over_base_names() {
        assert_eq!(chip_rates(Some("Apple M4 Pro")).prefill, 210.0);
        assert_eq!(chip_rates(Some("Apple M4")).prefill, 105.0);
        assert_eq!(chip_rates(Some("Apple M2 Max")).decode, 35.0);
        assert_eq!(chip_rates(Some("Apple M5")).prefill, 350.0);
    }

    #[test]
    fn skill_user_passes_short_content_through() {
        let (prompt, truncated) = skill_user("short body");
        assert!(prompt.contains("short body"));
        assert!(!truncated);
        assert!(!prompt.contains(TRUNCATION_MARKER.trim()));
    }

    #[test]
    fn skill_user_truncates_past_the_cap_with_marker() {
        // Roughly one token per word; 9000 words clears the 6000 cap.
        let big = "word ".repeat(9000);
        let (prompt, truncated) = skill_user(&big);
        assert!(truncated);
        assert!(prompt.contains("[truncated by tolkin at ~6000 tokens]"));
        assert!(prompt.len() < big.len());
    }

    #[test]
    fn retry_prompts_keep_the_static_prefix_first() {
        assert!(skill_retry_system().starts_with(SKILL_SYSTEM));
        let retry = narrate_retry_system(&["42".to_string(), "7".to_string()]);
        assert!(retry.starts_with(NARRATE_SYSTEM));
        assert!(retry.contains("42, 7"));
    }

    #[test]
    fn skill_schema_lists_both_enums() {
        let schema = skill_schema();
        let text = schema.to_string();
        assert!(text.contains("trigger"));
        assert!(text.contains("high"));
        assert!(text.contains("evidence_quote"));
    }
}
