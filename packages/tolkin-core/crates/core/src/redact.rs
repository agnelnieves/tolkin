//! Always-on secret redaction. Runs before display, analysis, or any opt-in
//! network call. Ships in the CLI natively and in the browser via WASM so both
//! surfaces share one pattern catalog. See PLAN.md section 7.
//!
//! Confidence model: 0.4 base, plus 0.3 for a vendor regex match, plus 0.2 for
//! keyword context, plus 0.1 for the entropy gate. At or above the threshold a
//! finding is auto-redacted; below it the finding is surfaced for review but the
//! text is left untouched. Placeholders are a single deterministic label per
//! type so token counts stay stable and the original secret length is not leaked.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Confidence at or above this is auto-redacted. `--strict` raises the bar so
/// only high-confidence matches redact (fewer, higher-precision redactions).
const THRESHOLD: f64 = 0.5;
const STRICT_THRESHOLD: f64 = 0.7;

/// Bytes scanned before a match for keyword context ("key", "token", ...).
const KEYWORD_WINDOW: usize = 32;

/// One detection in the input.
#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    /// Catalog id, e.g. "openai-key" or "high-entropy".
    pub kind: String,
    /// Deterministic placeholder substituted for the secret.
    pub label: String,
    /// Byte offsets into the original input.
    pub start: usize,
    pub end: usize,
    /// 0.0 to 1.0 detection confidence.
    pub confidence: f64,
    /// Whether this finding was auto-redacted. `false` means it was left in the
    /// text and surfaced for review (below threshold, or allow-listed).
    pub redacted: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RedactOptions {
    /// Raise the confidence threshold so only high-confidence matches redact.
    #[serde(default)]
    pub strict: bool,
    /// Catalog kinds to leave untouched (false-positive escape hatch).
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RedactionResult {
    pub redacted_text: String,
    pub findings: Vec<Finding>,
    pub redacted_count: usize,
    pub review_count: usize,
}

struct Pattern {
    kind: &'static str,
    /// Capture group whose span is the secret (0 = whole match).
    group: usize,
    /// A specific-format vendor pattern (adds the regex-match confidence band).
    vendor: bool,
    /// Structural rule (config / arg / header): the key name is the keyword
    /// context, so treat it as keyword-adjacent regardless of surroundings.
    structural: bool,
    re: Regex,
}

fn vendor(kind: &'static str, re: &str) -> Pattern {
    Pattern {
        kind,
        group: 0,
        vendor: true,
        structural: false,
        re: Regex::new(re).expect("static vendor regex"),
    }
}

fn structural(kind: &'static str, group: usize, re: &str) -> Pattern {
    Pattern {
        kind,
        group,
        vendor: false,
        structural: true,
        re: Regex::new(re).expect("static structural regex"),
    }
}

fn patterns() -> &'static [Pattern] {
    static PATTERNS: OnceLock<Vec<Pattern>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            vec![
                vendor("openai-key", r"\bsk-(?:proj|svcacct|admin)-[A-Za-z0-9_-]{20,}\b"),
                vendor("openai-key", r"\bsk-[A-Za-z0-9]{20,}\b"),
                vendor("anthropic-key", r"\bsk-ant-(?:api03|admin01|oat01)-[A-Za-z0-9_-]{20,}\b"),
                vendor("google-ai-key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
                vendor("aws-access-key", r"\b(?:AKIA|ASIA|AROA|AIDA|ANPA|ANVA|AIPA)[0-9A-Z]{16}\b"),
                vendor("github-token", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}\b"),
                vendor("github-token", r"\bgithub_pat_[A-Za-z0-9_]{22,}\b"),
                vendor("slack-token", r"\bxox[baprsoe]-[A-Za-z0-9-]{10,}\b"),
                vendor("slack-webhook", r"https://hooks\.slack\.com/services/[A-Za-z0-9/]+"),
                vendor("stripe-key", r"\b(?:sk|rk|pk)_live_[A-Za-z0-9]{20,}\b"),
                vendor("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
                vendor("private-key", r"-----BEGIN (?:RSA |EC |OPENSSH |PGP |DSA )?PRIVATE KEY-----"),
                vendor("db-uri", r"\b(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?)://[^\s:@/]+:[^\s:@/]+@[^\s/]+"),
                vendor("notion-key", r"\b(?:secret_|ntn_)[A-Za-z0-9]{36,}\b"),
                vendor("linear-key", r"\blin_api_[A-Za-z0-9]{32,}\b"),
                vendor("figma-token", r"\bfigd_[A-Za-z0-9_-]{32,}\b"),
                vendor("neon-key", r"\bnapi_[A-Za-z0-9]{32,}\b"),
                vendor("supabase-token", r"\bsbp_[A-Za-z0-9]{36,}\b"),
                // Structural (value-position) rules. The matched key name supplies
                // the keyword context; the captured value is what gets redacted.
                structural(
                    "config-secret",
                    2,
                    r#"(?i)"([a-z0-9_.\- ]*(?:api[_-]?key|secret|token|password|passwd|auth)[a-z0-9_.\- ]*)"\s*:\s*"([^"]{6,})""#,
                ),
                structural(
                    "arg-secret",
                    1,
                    r#"(?i)--(?:api[-_]?key|token|secret|password|auth)[ =]+"?([^\s"']{6,})"#,
                ),
                structural(
                    "auth-header",
                    1,
                    r#"(?i)authorization"?\s*[:=]\s*"?(?:bearer\s+)?([A-Za-z0-9._\-]{16,})"#,
                ),
            ]
        })
        .as_slice()
}

struct Candidate {
    kind: &'static str,
    start: usize,
    end: usize,
    confidence: f64,
}

/// Redact secrets in `text`, returning the cleaned text plus a ledger.
pub fn redact(text: &str, opts: &RedactOptions) -> RedactionResult {
    let threshold = if opts.strict {
        STRICT_THRESHOLD
    } else {
        THRESHOLD
    };

    let mut cands = collect(text);
    // Resolve overlaps: keep the highest-confidence (then longest) candidate and
    // drop anything overlapping it. This prefers a specific vendor match over a
    // generic high-entropy match covering the same span.
    cands.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then((b.end - b.start).cmp(&(a.end - a.start)))
    });
    let mut accepted: Vec<Candidate> = Vec::new();
    for c in cands {
        if accepted.iter().any(|a| a.start < c.end && c.start < a.end) {
            continue;
        }
        accepted.push(c);
    }
    accepted.sort_by_key(|c| c.start);

    let mut findings = Vec::with_capacity(accepted.len());
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut redacted_count = 0usize;
    let mut review_count = 0usize;

    for c in &accepted {
        let allowed = opts.allow.iter().any(|k| k == c.kind);
        let redacted = c.confidence >= threshold && !allowed;
        let label = format!("<REDACTED:{}>", c.kind);
        if redacted {
            out.push_str(&text[cursor..c.start]);
            out.push_str(&label);
            cursor = c.end;
            redacted_count += 1;
        } else {
            review_count += 1;
        }
        findings.push(Finding {
            kind: c.kind.to_string(),
            label,
            start: c.start,
            end: c.end,
            confidence: (c.confidence * 100.0).round() / 100.0,
            redacted,
        });
    }
    out.push_str(&text[cursor..]);

    RedactionResult {
        redacted_text: out,
        findings,
        redacted_count,
        review_count,
    }
}

fn collect(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();

    for p in patterns() {
        for caps in p.re.captures_iter(text) {
            let Some(m) = caps.get(p.group) else { continue };
            let (start, end) = (m.start(), m.end());
            let slice = &text[start..end];
            if is_false_positive(slice) {
                continue;
            }
            let keyword = p.structural || keyword_near(text, start);
            let mut confidence = 0.4;
            if p.vendor {
                confidence += 0.3;
            }
            if keyword {
                confidence += 0.2;
            }
            if entropy_pass(slice) {
                confidence += 0.1;
            }
            out.push(Candidate {
                kind: p.kind,
                start,
                end,
                confidence,
            });
        }
    }

    // Generic high-entropy / unknown-format secrets.
    static GENERIC: OnceLock<Regex> = OnceLock::new();
    let re =
        GENERIC.get_or_init(|| Regex::new(r"[A-Za-z0-9+/=_-]{24,}").expect("static generic regex"));
    for m in re.find_iter(text) {
        let (start, end) = (m.start(), m.end());
        let slice = &text[start..end];
        if is_false_positive(slice) {
            continue;
        }
        let keyword = keyword_near(text, start);
        // Bare hex blobs (git SHAs, content hashes) are noise unless a keyword
        // sits next to them, so only credit entropy for hex when keyword-adjacent.
        let entropy = if is_hex(slice) {
            keyword && entropy_pass(slice)
        } else {
            entropy_pass(slice)
        };
        let mut confidence = 0.4;
        if keyword {
            confidence += 0.2;
        }
        if entropy {
            confidence += 0.1;
        }
        out.push(Candidate {
            kind: "high-entropy",
            start,
            end,
            confidence,
        });
    }

    out
}

fn keyword_near(text: &str, start: usize) -> bool {
    const KEYWORDS: [&str; 7] = [
        "key", "token", "secret", "auth", "password", "api_key", "apikey",
    ];
    let from = floor_char_boundary(text, start.saturating_sub(KEYWORD_WINDOW));
    let window = text[from..start].to_ascii_lowercase();
    KEYWORDS.iter().any(|k| window.contains(k))
}

fn floor_char_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Shannon-entropy gate. Base64-ish strings need >= 4.0 bits/char; hex strings
/// (lower max entropy) need >= 2.7 and at least 32 chars. Short strings fail.
fn entropy_pass(s: &str) -> bool {
    if s.len() < 16 {
        return false;
    }
    let bits = shannon_bits_per_char(s);
    if is_hex(s) {
        s.len() >= 32 && bits >= 2.7
    } else {
        bits >= 4.0
    }
}

fn shannon_bits_per_char(s: &str) -> f64 {
    let mut counts: HashMap<u8, u32> = HashMap::new();
    for b in s.bytes() {
        *counts.entry(b).or_insert(0) += 1;
    }
    let len = s.len() as f64;
    let mut bits = 0.0;
    for &count in counts.values() {
        let p = count as f64 / len;
        bits -= p * p.log2();
    }
    bits
}

fn is_false_positive(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    const PLACEHOLDERS: [&str; 15] = [
        "your_api_key",
        "your-api-key",
        "yourapikey",
        "xxxx",
        "replace_me",
        "replaceme",
        "example",
        "changeme",
        "placeholder",
        "lorem",
        "ipsum",
        "dummy",
        "sample",
        "redacted",
        "<your",
    ];
    if PLACEHOLDERS.iter().any(|p| lower.contains(p)) {
        return true;
    }
    if lower.starts_with("data:image/") {
        return true;
    }
    if let Some(first) = s.bytes().next() {
        if s.bytes().all(|b| b == first) {
            return true;
        }
    }
    is_uuid(s)
}

fn is_uuid(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
            .expect("static uuid regex")
    })
    .is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red(t: &str) -> RedactionResult {
        redact(t, &RedactOptions::default())
    }

    #[test]
    fn redacts_openai_key() {
        let r = red("here is my key sk-Ab12Cd34Ef56Gh78Ij90Kl12 ok");
        assert!(
            r.redacted_text.contains("<REDACTED:openai-key>"),
            "{}",
            r.redacted_text
        );
        assert!(!r.redacted_text.contains("sk-Ab12Cd34Ef56Gh78Ij90Kl12"));
        assert_eq!(r.redacted_count, 1);
    }

    #[test]
    fn redacts_private_key_header() {
        let r = red("-----BEGIN RSA PRIVATE KEY-----\nMIIEv...");
        assert!(r.redacted_text.contains("<REDACTED:private-key>"));
    }

    #[test]
    fn redacts_db_uri_with_password() {
        let r = red("DATABASE_URL=postgres://admin:hunter2pass@prod-db.acme.io:5432/app");
        assert!(
            r.redacted_text.contains("<REDACTED:db-uri>"),
            "{}",
            r.redacted_text
        );
    }

    #[test]
    fn config_secret_value_redacted_key_name_kept() {
        let r = red(r#"{"api_key": "abcd1234efgh5678ijkl"}"#);
        assert!(
            r.redacted_text.contains("<REDACTED:config-secret>"),
            "{}",
            r.redacted_text
        );
        assert!(!r.redacted_text.contains("abcd1234efgh5678ijkl"));
        assert!(r.redacted_text.contains("api_key"));
    }

    #[test]
    fn placeholder_not_redacted() {
        let r = red(r#"{"api_key": "YOUR_API_KEY_HERE"}"#);
        assert!(r.redacted_text.contains("YOUR_API_KEY_HERE"));
        assert_eq!(r.redacted_count, 0);
    }

    #[test]
    fn prose_is_untouched() {
        let t = "The quick brown fox jumps over the lazy dog repeatedly today.";
        let r = red(t);
        assert_eq!(r.redacted_text, t);
        assert_eq!(r.redacted_count, 0);
    }

    #[test]
    fn uuid_not_flagged() {
        let r = red("request id 550e8400-e29b-41d4-a716-446655440000 logged");
        assert_eq!(r.redacted_count, 0);
    }

    #[test]
    fn allow_list_leaves_secret_but_surfaces_review() {
        let opts = RedactOptions {
            strict: false,
            allow: vec!["openai-key".to_string()],
        };
        let r = redact("key sk-Ab12Cd34Ef56Gh78Ij90Kl12", &opts);
        assert!(r.redacted_text.contains("sk-Ab12Cd34Ef56Gh78Ij90Kl12"));
        assert_eq!(r.redacted_count, 0);
        assert!(r
            .findings
            .iter()
            .any(|f| f.kind == "openai-key" && !f.redacted));
    }

    #[test]
    fn strict_demotes_borderline_entropy_to_review() {
        // A bare high-entropy base64 token, no keyword: base 0.4 + entropy 0.1 = 0.5.
        let t = "blob Zm9vYmFyYmF6cXV4Y29ycmVjdABCDEF1 trailing";
        let normal = redact(t, &RedactOptions::default());
        assert_eq!(normal.redacted_count, 1);
        let strict = redact(
            t,
            &RedactOptions {
                strict: true,
                allow: vec![],
            },
        );
        assert_eq!(strict.redacted_count, 0);
        assert_eq!(strict.review_count, 1);
    }

    #[test]
    fn label_is_deterministic_per_kind() {
        let r = red("a sk-Ab12Cd34Ef56Gh78Ij90Kl12 and b sk-Zz99Yy88Xx77Ww66Vv55Uu44");
        let labels: Vec<&str> = r.findings.iter().map(|f| f.label.as_str()).collect();
        assert!(labels.iter().all(|l| *l == "<REDACTED:openai-key>"));
    }

    #[test]
    fn findings_serialize_to_json() {
        let r = red("key sk-Ab12Cd34Ef56Gh78Ij90Kl12");
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"redacted_text\""));
        assert!(json.contains("\"findings\""));
    }
}
