//! Verification gates for model advisory output. Pure functions only:
//! no IO, no network, no env. The model proposes, these functions verify.
//!
//! Two gates ship in v1:
//! - number faithfulness: every digit run in narration output must already
//!   exist in the deterministic input JSON (or be a small whitelisted value),
//! - span grounding: every skill-lint evidence quote must be a verbatim
//!   substring of the (redacted) file the model saw.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Strip grouping commas that sit between two digits ("14,989" -> "14989")
/// so formatted numbers compare against raw JSON digits.
fn strip_grouping_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let between_digits = c == ','
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1].is_ascii_digit()
            && chars[i + 1].is_ascii_digit();
        if between_digits {
            continue;
        }
        out.push(c);
    }
    out
}

/// All maximal runs of ascii digits in `text`, after comma normalization.
pub fn digit_runs(text: &str) -> Vec<String> {
    let cleaned = strip_grouping_commas(text);
    let mut runs = Vec::new();
    let mut cur = String::new();
    for c in cleaned.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs
}

/// Number-faithfulness gate. Returns the digit runs in `output` that do not
/// appear as digit runs in `input_json` and are not whitelisted (0 to 10 and
/// 100, the values prose naturally uses for lists and percent framing).
/// Empty result means the narration passed.
pub fn number_violations(output: &str, input_json: &str) -> Vec<String> {
    let allowed: HashSet<String> = digit_runs(input_json).into_iter().collect();
    let mut violations: Vec<String> = Vec::new();
    for run in digit_runs(output) {
        if allowed.contains(&run) {
            continue;
        }
        if let Ok(v) = run.parse::<u64>() {
            if v <= 10 || v == 100 {
                continue;
            }
        }
        if !violations.contains(&run) {
            violations.push(run);
        }
    }
    violations
}

/// Span-grounding gate: the quote (whitespace-trimmed) must appear verbatim
/// in the content the model was shown. Empty quotes never ground.
pub fn quote_grounded(quote: &str, content: &str) -> bool {
    let q = quote.trim();
    !q.is_empty() && content.contains(q)
}

/// Rubric aspects the skill lint may report. Anything else fails parsing.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Aspect {
    Trigger,
    Verbosity,
    Examples,
    Structure,
}

/// Severity enum, identical to the deterministic rules' vocabulary but kept
/// separate: advisory findings are never merged with tier findings.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SkillFinding {
    pub aspect: Aspect,
    pub severity: Severity,
    pub evidence_quote: String,
    pub suggestion: String,
}

#[derive(Debug, Deserialize)]
struct RawSkillPayload {
    #[serde(default)]
    findings: Vec<SkillFinding>,
    #[serde(default)]
    summary: String,
}

/// A skill-lint payload that survived the gates. `dropped_findings` counts
/// findings discarded because their evidence quote was not grounded.
#[derive(Clone, Debug)]
pub struct ValidatedSkillLint {
    pub summary: String,
    pub findings: Vec<SkillFinding>,
    pub dropped_findings: usize,
}

/// Parse and gate one skill-lint JSON object against the redacted file
/// content the model saw. Schema violations (bad enums, wrong shapes) fail
/// the whole payload; ungrounded quotes drop individual findings.
pub fn validate_skill_payload(
    raw_json: &str,
    redacted_file: &str,
) -> Result<ValidatedSkillLint, String> {
    let parsed: RawSkillPayload =
        serde_json::from_str(raw_json).map_err(|e| format!("schema validation failed: {e}"))?;
    let mut findings = Vec::new();
    let mut dropped_findings = 0;
    for f in parsed.findings {
        if quote_grounded(&f.evidence_quote, redacted_file) {
            findings.push(f);
        } else {
            dropped_findings += 1;
        }
    }
    let mut summary = parsed.summary;
    if summary.chars().count() > 200 {
        summary = summary.chars().take(200).collect();
    }
    Ok(ValidatedSkillLint {
        summary,
        findings,
        dropped_findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT: &str = r#"{"totals":{"files_scanned":214,"context_files":4,"context_tokens":14989},"heaviest":[{"path":"CLAUDE.md","tokens":3816}]}"#;

    #[test]
    fn faithful_narration_passes() {
        let out = "You have 214 files scanned and 4 context files costing 14989 tokens. \
                   CLAUDE.md alone is 3816 tokens. Top 3 actions follow.";
        assert!(number_violations(out, INPUT).is_empty());
    }

    #[test]
    fn fabricated_number_is_caught() {
        let out = "This costs you roughly 5000 tokens per session, a 37 percent overhead.";
        let v = number_violations(out, INPUT);
        assert!(v.contains(&"5000".to_string()));
        assert!(v.contains(&"37".to_string()));
    }

    #[test]
    fn comma_formatted_numbers_normalize_before_matching() {
        let out = "Your agent context totals 14,989 tokens across 4 files.";
        assert!(number_violations(out, INPUT).is_empty());
    }

    #[test]
    fn whitelist_allows_small_counts_and_100() {
        let out = "Here are 3 actions. First, cut 100 percent of the bloat in 2 files. 0 risk.";
        assert!(number_violations(out, INPUT).is_empty());
        // 11 is past the whitelist and absent from the input.
        let v = number_violations("Try these 11 steps.", INPUT);
        assert_eq!(v, vec!["11".to_string()]);
    }

    #[test]
    fn violations_deduplicate() {
        let out = "About 42 here and 42 there.";
        assert_eq!(number_violations(out, INPUT), vec!["42".to_string()]);
    }

    #[test]
    fn quote_grounding_verbatim_passes() {
        let file = "---\nname: demo\n---\nUse this skill when deploying.\n";
        assert!(quote_grounded("Use this skill when deploying.", file));
    }

    #[test]
    fn quote_grounding_is_trim_insensitive() {
        let file = "Line one.\nLine two stands here.\n";
        assert!(quote_grounded("  Line two stands here.  ", file));
    }

    #[test]
    fn quote_grounding_rejects_absent_and_empty_quotes() {
        let file = "Only this text exists.";
        assert!(!quote_grounded("invented evidence", file));
        assert!(!quote_grounded("", file));
        assert!(!quote_grounded("   ", file));
    }

    #[test]
    fn validate_keeps_grounded_findings_and_drops_the_rest() {
        let file = "---\nname: demo\ndescription: does things\n---\nbody text here\n";
        let raw = r#"{"findings":[
            {"aspect":"trigger","severity":"high","evidence_quote":"description: does things","suggestion":"name the exact trigger phrases"},
            {"aspect":"verbosity","severity":"low","evidence_quote":"this text is not in the file","suggestion":"trim it"}
        ],"summary":"one vague trigger"}"#;
        let v = validate_skill_payload(raw, file).unwrap();
        assert_eq!(v.findings.len(), 1);
        assert_eq!(v.dropped_findings, 1);
        assert_eq!(v.findings[0].aspect, Aspect::Trigger);
        assert_eq!(v.findings[0].severity, Severity::High);
        assert_eq!(v.summary, "one vague trigger");
    }

    #[test]
    fn validate_rejects_bad_severity_enum() {
        let raw = r#"{"findings":[{"aspect":"trigger","severity":"catastrophic","evidence_quote":"x","suggestion":"y"}],"summary":"s"}"#;
        assert!(validate_skill_payload(raw, "x").is_err());
    }

    #[test]
    fn validate_rejects_bad_aspect_enum_and_clamps_long_summary() {
        let raw = r#"{"findings":[{"aspect":"vibes","severity":"low","evidence_quote":"x","suggestion":"y"}],"summary":"s"}"#;
        assert!(validate_skill_payload(raw, "x").is_err());

        let long = "a".repeat(300);
        let raw_ok = format!(r#"{{"findings":[],"summary":"{long}"}}"#);
        let v = validate_skill_payload(&raw_ok, "x").unwrap();
        assert_eq!(v.summary.chars().count(), 200);
    }
}
