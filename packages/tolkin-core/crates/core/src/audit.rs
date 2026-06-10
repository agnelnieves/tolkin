//! Audit rules engine: a Lighthouse-style ranked scan for token waste. The
//! production-proven detections from PLAN.md section 8 always run; the
//! experimental detections (higher false-positive risk) only run when the
//! caller opts in via `AuditOptions::include_experimental` and carry an
//! "experimental" badge. Each finding carries a severity, an input-token
//! savings range, a confidence score, and a citation. Tokenization is
//! intentionally NOT done here: the caller passes a real token count when it
//! has one, otherwise the engine falls back to a bytes/4 approximation and
//! says so in the report notes.
//!
//! All savings are input-token-bounded. The report always carries the
//! "output may vary" note because aggressive compression has been measured to
//! increase total cost when output grows (see PLAN.md section 8).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::format::{self, FormatPreview};

/// Low false-positive risk rules that always run.
const BADGE: &str = "production-proven";
/// Opt-in rules with higher false-positive risk; gated behind
/// `AuditOptions::include_experimental`.
const BADGE_EXPERIMENTAL: &str = "experimental";
const EXPERIMENTAL_NOTE: &str =
    "Experimental findings have higher false-positive risk; review before acting.";

/// Paragraphs shorter than this never enter the near-duplicate pass.
const MIN_PARA_BYTES: usize = 200;
/// MinHash family size. Estimated Jaccard resolves in steps of 1/64, which is
/// plenty for a 0.7 threshold.
const MINHASH_FNS: usize = 64;
const SHINGLE_BYTES: usize = 5;
const JACCARD_THRESHOLD: f64 = 0.7;
/// Brace-balanced spans at or below this are too small to be worth minifying.
const MIN_JSON_BYTES: usize = 300;
/// Consecutive stack-frame lines required before a run counts as a trace.
const MIN_TRACE_FRAMES: usize = 6;
/// Frames an agent actually needs from a trace; everything past these is fat.
const KEPT_FRAMES: usize = 5;
/// Anthropic and OpenAI both gate prompt caching at 1024 tokens.
const CACHE_MIN_TOKENS: u64 = 1024;
/// Filler hits below this are normal prose, not a pattern worth flagging.
const MIN_FILLER_HITS: usize = 5;
/// System prompts live at the head of the input; sentence-level dedup only
/// scans this zone to keep the false-positive rate down on long documents.
const SYSTEM_ZONE_BYTES: usize = 4096;
/// Sentences shorter than this are too generic for near-dup comparison.
const MIN_SENTENCE_BYTES: usize = 40;
/// Word-set Jaccard threshold for "the same instruction, rephrased".
const SENTENCE_JACCARD: f64 = 0.8;
/// Role/persona descriptions concentrate in the first 2KB.
const ROLE_ZONE_BYTES: usize = 2048;
/// Consecutive role-pattern sentences tolerated before flagging.
const MAX_ROLE_SENTENCES: usize = 3;
/// Few-shot example blocks tolerated before the plateau literature says the
/// marginal example stops paying for itself.
const MAX_EXAMPLE_BLOCKS: usize = 5;
/// Markdown markup share of total bytes above which plain text may win.
const MARKDOWN_OVERHEAD_PCT: usize = 15;
/// "Lost in the middle" effects are measured on long contexts only.
const LITM_MIN_TOKENS: u64 = 50_000;

const CITE_DEDUP: &str = "https://arxiv.org/abs/2310.04408";
const CITE_JSON: &str = "https://arxiv.org/abs/2508.13666";
const CITE_CONTEXT_ENG: &str =
    "https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents";
const CITE_ANTHROPIC_CACHE: &str =
    "https://platform.claude.com/docs/en/build-with-claude/prompt-caching";
const CITE_OPENAI_CACHE: &str =
    "https://developers.openai.com/cookbook/examples/prompt_caching_201";
const CITE_REPEATED_INSTRUCTIONS: &str = "https://arxiv.org/abs/2509.14404";
const CITE_ROLE: &str = "https://arxiv.org/abs/2505.12592";
const CITE_FEW_SHOT: &str = "https://arxiv.org/abs/2312.08901";
const CITE_LITM: &str = "https://arxiv.org/abs/2307.03172";
const CITE_TOON: &str = "https://arxiv.org/abs/2603.03306";

/// How the audit should interpret the input.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct AuditOptions {
    /// Token count of the text if the caller already tokenized it.
    /// `None` falls back to a bytes/4 approximation, labeled in the notes.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    /// Opt in to the experimental rules (higher false-positive risk). Their
    /// findings carry the "experimental" badge and the report gains a
    /// review-before-acting note.
    #[serde(default)]
    pub include_experimental: bool,
}

/// One detected source of token waste.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    /// Kebab-case rule id, e.g. "near-duplicate-paragraphs".
    pub rule: String,
    /// "high" | "medium" | "low".
    pub severity: String,
    pub title: String,
    /// Specific to what was found, with counts.
    pub detail: String,
    /// Byte offsets of the representative span in the original input.
    pub byte_start: usize,
    pub byte_end: usize,
    /// Estimated input-token savings range.
    pub savings_min: u64,
    pub savings_max: u64,
    /// 0..1 detection confidence.
    pub confidence: f32,
    pub badge: String,
    /// URL backing the detection.
    pub citation: String,
    /// Converted-format preview, attached when a rule can show the exact
    /// cheaper representation. Skipped in JSON when absent so findings
    /// without previews keep their existing shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<FormatPreview>,
}

/// The full audit result: findings ranked by severity then savings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    /// Sum of every finding's savings range (input tokens).
    pub total_savings_min: u64,
    pub total_savings_max: u64,
    pub notes: Vec<String>,
}

/// Bytes-to-tokens estimate used for byte spans and as the whole-input
/// fallback when the caller did not tokenize. Four bytes per token is the
/// standard rough average for English prose on modern BPE vocabularies.
pub fn approx_tokens(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(4)
}

/// Run every production-proven rule over `text` (plus the experimental rules
/// when opted in) and rank the findings.
pub fn audit(text: &str, options: &AuditOptions) -> AuditReport {
    let mut findings = Vec::new();

    if !text.is_empty() {
        let total_tokens = options
            .input_tokens
            .unwrap_or_else(|| approx_tokens(text.len()));
        detect_near_duplicates(text, &mut findings);
        detect_json_verbosity(text, options.include_experimental, &mut findings);
        detect_stack_traces(text, &mut findings);
        detect_volatile_prefix(text, total_tokens, &mut findings);
        detect_sub_cache_threshold(text, options, total_tokens, &mut findings);
        detect_html_content(text, &mut findings);
        if options.include_experimental {
            detect_filler_phrases(text, total_tokens, &mut findings);
            detect_repeated_instructions(text, &mut findings);
            detect_verbose_role_description(text, &mut findings);
            detect_excessive_few_shot(text, &mut findings);
            detect_markdown_overhead(text, total_tokens, &mut findings);
            detect_lost_in_the_middle(text, total_tokens, &mut findings);
        }
    }

    findings.sort_by(|a, b| {
        severity_rank(&a.severity)
            .cmp(&severity_rank(&b.severity))
            .then(b.savings_max.cmp(&a.savings_max))
    });

    let mut notes = vec!["Savings are input-token estimates; output may vary.".to_string()];
    if options.input_tokens.is_none() {
        notes.push("Token figures use a bytes/4 approximation.".to_string());
    }
    if findings.iter().any(|f| f.badge == BADGE_EXPERIMENTAL) {
        notes.push(EXPERIMENTAL_NOTE.to_string());
    }

    AuditReport {
        total_savings_min: findings.iter().map(|f| f.savings_min).sum(),
        total_savings_max: findings.iter().map(|f| f.savings_max).sum(),
        findings,
        notes,
    }
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

// ---------------------------------------------------------------------------
// near-duplicate-paragraphs
// ---------------------------------------------------------------------------

struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

/// Maximal runs of non-blank lines, with byte offsets into `text`.
fn paragraphs(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0usize;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            if let Some(s) = start.take() {
                out.push(Span { start: s, end });
            }
        } else {
            if start.is_none() {
                start = Some(offset);
            }
            end = offset + line.trim_end().len();
        }
        offset += line.len();
    }
    if let Some(s) = start {
        out.push(Span { start: s, end });
    }
    out
}

/// splitmix64: both the seed generator and the per-function mixer. Public
/// domain construction; statistically strong enough for MinHash permutations
/// without pulling in a hashing dependency.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn minhash_seeds() -> &'static [u64; MINHASH_FNS] {
    static SEEDS: OnceLock<[u64; MINHASH_FNS]> = OnceLock::new();
    SEEDS.get_or_init(|| {
        let mut seeds = [0u64; MINHASH_FNS];
        let mut state = 0x70_6B_65_6E_6C_79u64; // "tolkin", any fixed seed works
        for s in seeds.iter_mut() {
            state = splitmix64(state);
            *s = state;
        }
        seeds
    })
}

fn minhash_signature(bytes: &[u8]) -> [u64; MINHASH_FNS] {
    let seeds = minhash_seeds();
    let mut sig = [u64::MAX; MINHASH_FNS];
    for window in bytes.windows(SHINGLE_BYTES) {
        let base = fnv1a(window);
        for (slot, seed) in sig.iter_mut().zip(seeds.iter()) {
            let h = splitmix64(base ^ seed);
            if h < *slot {
                *slot = h;
            }
        }
    }
    sig
}

fn estimated_jaccard(a: &[u64; MINHASH_FNS], b: &[u64; MINHASH_FNS]) -> f64 {
    let equal = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    equal as f64 / MINHASH_FNS as f64
}

fn uf_find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn detect_near_duplicates(text: &str, findings: &mut Vec<Finding>) {
    let paras: Vec<Span> = paragraphs(text)
        .into_iter()
        .filter(|p| p.len() >= MIN_PARA_BYTES)
        .collect();
    if paras.len() < 2 {
        return;
    }

    let sigs: Vec<[u64; MINHASH_FNS]> = paras
        .iter()
        .map(|p| minhash_signature(&text.as_bytes()[p.start..p.end]))
        .collect();

    let mut parent: Vec<usize> = (0..paras.len()).collect();
    for i in 0..paras.len() {
        for j in (i + 1)..paras.len() {
            if estimated_jaccard(&sigs[i], &sigs[j]) >= JACCARD_THRESHOLD {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    // Group members per root. BTreeMap keeps cluster order deterministic, and
    // members are pushed in document order.
    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..paras.len() {
        let root = uf_find(&mut parent, i);
        clusters.entry(root).or_default().push(i);
    }

    for members in clusters.values().filter(|m| m.len() >= 2) {
        let smallest = members
            .iter()
            .map(|&m| approx_tokens(paras[m].len()))
            .min()
            .unwrap_or(0);
        let extra = (members.len() - 1) as u64;
        let savings_max = smallest * extra;
        let savings_min = savings_max / 2;
        let severity = if savings_max > 500 { "high" } else { "medium" };
        let copies = if extra == 1 {
            "1 extra copy".to_string()
        } else {
            format!("{extra} extra copies")
        };
        let rep = &paras[members[1]];
        findings.push(Finding {
            rule: "near-duplicate-paragraphs".to_string(),
            severity: severity.to_string(),
            title: "Near-duplicate paragraphs".to_string(),
            detail: format!(
                "{} paragraphs are near-duplicates (estimated Jaccard >= 0.7): {copies} of roughly {smallest} tokens each. Keep one copy and reference it.",
                members.len()
            ),
            byte_start: rep.start,
            byte_end: rep.end,
            savings_min,
            savings_max,
            confidence: 0.85,
            badge: BADGE.to_string(),
            citation: CITE_DEDUP.to_string(),
            preview: None,
        });
    }
}

// ---------------------------------------------------------------------------
// json-verbosity
// ---------------------------------------------------------------------------

/// End of the brace-balanced span starting at `start` (exclusive), or `None`
/// if it never closes. String-aware so braces inside values do not count.
fn balanced_span_end(b: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i64;
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in b.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth <= 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Bytes a minifier would strip from a JSON-looking span without parsing it:
/// leading indentation plus the newline on every line.
fn removable_whitespace(span: &str) -> usize {
    span.lines()
        .map(|l| l.len() - l.trim_start().len() + 1)
        .sum()
}

fn detect_json_verbosity(text: &str, include_experimental: bool, findings: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    while pos < text.len() {
        let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i + 1);
        let line = &text[pos..line_end];
        let indent = line.len() - line.trim_start().len();
        let first = line.trim_start().bytes().next();
        if matches!(first, Some(b'{') | Some(b'[')) {
            let start = pos + indent;
            if let Some(end) = balanced_span_end(bytes, start) {
                if end - start > MIN_JSON_BYTES {
                    let span = &text[start..end];
                    if push_json_finding(span, start, end, include_experimental, findings) {
                        pos = end;
                        continue;
                    }
                }
            }
        }
        pos = line_end;
    }
}

/// Returns true when the span was consumed (a finding was pushed or the span
/// parsed but had nothing to save), so the scanner can skip past it.
fn push_json_finding(
    span: &str,
    start: usize,
    end: usize,
    include_experimental: bool,
    findings: &mut Vec<Finding>,
) -> bool {
    let block_tokens = approx_tokens(span.len());
    let mut preview = None;
    let (saved_min, saved_max, confidence, exact) =
        match serde_json::from_str::<serde_json::Value>(span) {
            Ok(value) => {
                let minified = serde_json::to_string(&value).unwrap_or_default();
                let saved = approx_tokens(span.len().saturating_sub(minified.len()));
                preview = format::json_minify(span);
                if include_experimental {
                    push_toon_candidate(span, start, end, findings);
                }
                (saved, saved, 0.95, true)
            }
            Err(_) => {
                // Not strict JSON (JSONC, trailing commas, a log excerpt).
                // Only flag spans that look pretty-printed, and estimate from
                // the whitespace a minifier would strip.
                let key_lines = span.lines().filter(|l| l.contains("\": ")).count();
                if key_lines < 3 {
                    return false;
                }
                let saved = approx_tokens(removable_whitespace(span));
                (saved / 2, saved, 0.7, false)
            }
        };
    if saved_max == 0 {
        // Parsed but already compact: nothing to report, still skip the span.
        return exact;
    }
    let severity = if saved_max > 1000 { "high" } else { "medium" };
    let detail = if exact {
        format!(
            "A pretty-printed JSON block of about {block_tokens} tokens; re-serializing it compact saves {saved_max} tokens (measured by minifying)."
        )
    } else {
        format!(
            "An indented JSON-like block of about {block_tokens} tokens; minifying the whitespace would save roughly {saved_max} tokens."
        )
    };
    findings.push(Finding {
        rule: "json-verbosity".to_string(),
        severity: severity.to_string(),
        title: "Pretty-printed JSON".to_string(),
        detail,
        byte_start: start,
        byte_end: end,
        savings_min: saved_min,
        savings_max: saved_max,
        confidence,
        badge: BADGE.to_string(),
        citation: CITE_JSON.to_string(),
        preview,
    });
    true
}

// ---------------------------------------------------------------------------
// json-toon-candidate (experimental)
// ---------------------------------------------------------------------------

/// Emitted alongside json-verbosity when the span is a uniform array of flat
/// objects, the one shape where TOON's tabular form reliably beats compact
/// JSON. Savings are measured against the original span.
fn push_toon_candidate(span: &str, start: usize, end: usize, findings: &mut Vec<Finding>) {
    let Some(preview) = format::json_to_toon(span) else {
        return;
    };
    let saved = approx_tokens(preview.bytes_before.saturating_sub(preview.bytes_after));
    if saved == 0 {
        return;
    }
    findings.push(Finding {
        rule: "json-toon-candidate".to_string(),
        severity: "medium".to_string(),
        title: "Uniform JSON array could be TOON".to_string(),
        detail: format!(
            "A JSON array of uniform flat objects (about {} tokens); the TOON tabular form drops repeated keys and punctuation for roughly {saved} tokens saved versus the original (measured by converting).",
            approx_tokens(span.len())
        ),
        byte_start: start,
        byte_end: end,
        savings_min: saved / 2,
        savings_max: saved,
        confidence: 0.5,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_TOON.to_string(),
        preview: Some(preview),
    });
}

// ---------------------------------------------------------------------------
// stack-trace-verbosity
// ---------------------------------------------------------------------------

fn frame_regexes() -> &'static [Regex; 4] {
    static RES: OnceLock<[Regex; 4]> = OnceLock::new();
    RES.get_or_init(|| {
        [
            // Python: File "app.py", line 12
            Regex::new(r#"^\s*File "[^"]*", line \d+"#).expect("static frame regex"),
            // JS / Node: at handler (src/app.js:10:5)
            Regex::new(r"^\s*at .*\(.*:\d+:\d+\)").expect("static frame regex"),
            // Java: \tat com.acme.Service.run(
            Regex::new(r"^\tat [\w.$]+\(").expect("static frame regex"),
            // Rust backtrace: "  12: core::panicking..."
            Regex::new(r"^\s+\d+: ").expect("static frame regex"),
        ]
    })
}

fn detect_stack_traces(text: &str, findings: &mut Vec<Finding>) {
    let res = frame_regexes();
    let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut current: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if res.iter().any(|r| r.is_match(content)) {
            current.push((offset, offset + content.len()));
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
        offset += line.len();
    }
    if !current.is_empty() {
        runs.push(current);
    }

    for run in runs.iter().filter(|r| r.len() >= MIN_TRACE_FRAMES) {
        let trace_start = run[0].0;
        let trace_end = run[run.len() - 1].1;
        let tail_start = run[KEPT_FRAMES].0;
        let tail_tokens = approx_tokens(trace_end - tail_start);
        findings.push(Finding {
            rule: "stack-trace-verbosity".to_string(),
            severity: "high".to_string(),
            title: "Verbose stack trace".to_string(),
            detail: format!(
                "A stack trace with {} frames: the {} frames past the top {KEPT_FRAMES} ({tail_tokens} tokens) rarely add debugging signal and get re-sent on every turn.",
                run.len(),
                run.len() - KEPT_FRAMES
            ),
            byte_start: trace_start,
            byte_end: trace_end,
            savings_min: tail_tokens * 60 / 100,
            savings_max: tail_tokens * 80 / 100,
            confidence: 0.9,
            badge: BADGE.to_string(),
            citation: CITE_CONTEXT_ENG.to_string(),
            preview: None,
        });
    }
}

// ---------------------------------------------------------------------------
// volatile-prefix
// ---------------------------------------------------------------------------

fn volatile_patterns() -> &'static Vec<(&'static str, Regex)> {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (
                "an ISO timestamp",
                Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}").expect("static volatile regex"),
            ),
            (
                "a unix epoch timestamp",
                Regex::new(r"\b1[78]\d{8}\b").expect("static volatile regex"),
            ),
            (
                "a UUID",
                Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
                    .expect("static volatile regex"),
            ),
            (
                "a session-id-like hex value",
                Regex::new(r#"(?i)(?:session|request|trace)[ _-]?id["':=\s]{0,4}[0-9a-f]{16,}"#)
                    .expect("static volatile regex"),
            ),
        ]
    })
}

fn floor_char_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn detect_volatile_prefix(text: &str, total_tokens: u64, findings: &mut Vec<Finding>) {
    let limit = floor_char_boundary(text, text.len().min(1024));
    let prefix = &text[..limit];

    let mut kinds: Vec<&str> = Vec::new();
    let mut span: Option<(usize, usize)> = None;
    for (label, re) in volatile_patterns() {
        if let Some(m) = re.find(prefix) {
            kinds.push(label);
            if span.is_none_or(|(s, _)| m.start() < s) {
                span = Some((m.start(), m.end()));
            }
        }
    }
    let Some((start, end)) = span else { return };

    findings.push(Finding {
        rule: "volatile-prefix".to_string(),
        severity: "medium".to_string(),
        title: "Volatile prompt prefix".to_string(),
        detail: format!(
            "Found {} in the first 1KB. Volatile values at the head of a prompt invalidate the cache prefix, so every call re-pays the full input instead of reading up to 90% of the stable prefix from cache. Move them to the end.",
            kinds.join(", ")
        ),
        byte_start: start,
        byte_end: end,
        savings_min: 0,
        savings_max: total_tokens * 90 / 100,
        confidence: 0.6,
        badge: BADGE.to_string(),
        citation: CITE_ANTHROPIC_CACHE.to_string(),
            preview: None,
    });
}

// ---------------------------------------------------------------------------
// sub-cache-threshold
// ---------------------------------------------------------------------------

fn detect_sub_cache_threshold(
    text: &str,
    options: &AuditOptions,
    total_tokens: u64,
    findings: &mut Vec<Finding>,
) {
    if !(256..CACHE_MIN_TOKENS).contains(&total_tokens) {
        return;
    }
    // Only meaningful for prompts that get reused. Signals: a system-prompt
    // style opener, or the caller cared enough to tokenize it.
    let head = floor_char_boundary(text, text.len().min(200));
    let reused =
        text[..head].to_ascii_lowercase().contains("you are") || options.input_tokens.is_some();
    if !reused {
        return;
    }
    findings.push(Finding {
        rule: "sub-cache-threshold".to_string(),
        severity: "low".to_string(),
        title: "Prompt below the cache threshold".to_string(),
        detail: format!(
            "The prompt is about {total_tokens} tokens, below the {CACHE_MIN_TOKENS}-token prompt-cache minimum. Padding the stable prefix past the threshold (or merging it with other stable context) makes it cacheable."
        ),
        byte_start: 0,
        byte_end: text.len(),
        savings_min: 0,
        savings_max: total_tokens / 2,
        confidence: 0.5,
        badge: BADGE.to_string(),
        citation: CITE_OPENAI_CACHE.to_string(),
            preview: None,
    });
}

// ---------------------------------------------------------------------------
// html-content
// ---------------------------------------------------------------------------

fn detect_html_content(text: &str, findings: &mut Vec<Finding>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"</?[A-Za-z][A-Za-z0-9-]*(?:\s[^<>]*)?/?>").expect("static html regex")
    });

    let matches: Vec<regex::Match> = re.find_iter(text).collect();
    if matches.len() <= 20 {
        return;
    }
    let per_kb = matches.len() as f64 / (text.len() as f64 / 1024.0);
    if per_kb <= 5.0 {
        return;
    }

    let start = matches[0].start();
    let end = matches[matches.len() - 1].end();
    let region_tokens = approx_tokens(end - start);
    let severity = if region_tokens > 2000 {
        "high"
    } else {
        "medium"
    };

    // When the converter succeeds the savings stop being a heuristic range
    // and become the measured byte delta of the actual Markdown output.
    let preview = format::html_to_markdown(&text[start..end]);
    let measured = preview
        .as_ref()
        .map(|p| approx_tokens(p.bytes_before.saturating_sub(p.bytes_after)))
        .filter(|&saved| saved > 0);
    let (savings_min, savings_max, detail_suffix) = match measured {
        Some(saved) => (saved * 60 / 100, saved, " (measured by converting)"),
        None => (region_tokens * 20 / 100, region_tokens * 90 / 100, ""),
    };

    findings.push(Finding {
        rule: "html-content".to_string(),
        severity: severity.to_string(),
        title: "HTML content".to_string(),
        detail: format!(
            "{} HTML tags ({per_kb:.1} per KB) in a region of about {region_tokens} tokens. Converting HTML to Markdown strips tag and attribute boilerplate the model does not need{detail_suffix}.",
            matches.len()
        ),
        byte_start: start,
        byte_end: end,
        savings_min,
        savings_max,
        confidence: 0.8,
        badge: BADGE.to_string(),
        citation: CITE_CONTEXT_ENG.to_string(),
        preview,
    });
}

// ---------------------------------------------------------------------------
// Experimental rules. Everything below only runs when
// AuditOptions::include_experimental is set; every finding carries the
// "experimental" badge and a modest confidence because the false-positive
// rate is unproven on real-world corpora.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// filler-phrases (experimental)
// ---------------------------------------------------------------------------

fn filler_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // The single words only match standalone (word-bounded) so "justify"
        // or "everything" never count.
        Regex::new(
            r"(?i)please note that|it is important to|in order to|as you can see|keep in mind that|it should be noted|\b(?:basically|actually|very|really|just)\b",
        )
        .expect("static filler regex")
    })
}

fn detect_filler_phrases(text: &str, total_tokens: u64, findings: &mut Vec<Finding>) {
    let matches: Vec<regex::Match> = filler_regex().find_iter(text).collect();
    if matches.len() < MIN_FILLER_HITS {
        return;
    }
    findings.push(Finding {
        rule: "filler-phrases".to_string(),
        severity: "low".to_string(),
        title: "Filler and hedging phrases".to_string(),
        detail: format!(
            "{} filler or hedging phrases (\"please note that\", \"in order to\", standalone \"just\", and similar). They add tokens without adding instruction signal; cutting them typically trims 5-15% of prose.",
            matches.len()
        ),
        byte_start: matches[0].start(),
        byte_end: matches[0].end(),
        savings_min: total_tokens * 5 / 100,
        savings_max: total_tokens * 15 / 100,
        confidence: 0.5,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_CONTEXT_ENG.to_string(),
        preview: None,
    });
}

// ---------------------------------------------------------------------------
// repeated-instructions (experimental)
// ---------------------------------------------------------------------------

/// Sentence spans (byte offsets into `text`), split on period and newline
/// boundaries with surrounding whitespace trimmed off.
fn sentences(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut seg_start = 0usize;
    for (i, c) in text.char_indices() {
        if c == '.' || c == '\n' {
            push_trimmed(text, seg_start, i, &mut out);
            seg_start = i + c.len_utf8();
        }
    }
    push_trimmed(text, seg_start, text.len(), &mut out);
    out
}

fn push_trimmed(text: &str, start: usize, end: usize, out: &mut Vec<Span>) {
    let seg = &text[start..end];
    let trimmed = seg.trim_start();
    let new_start = start + (seg.len() - trimmed.len());
    let new_end = new_start + trimmed.trim_end().len();
    if new_end > new_start {
        out.push(Span {
            start: new_start,
            end: new_end,
        });
    }
}

fn word_set(s: &str) -> std::collections::BTreeSet<String> {
    s.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<std::collections::BTreeSet<String>>()
}

fn exact_jaccard(
    a: &std::collections::BTreeSet<String>,
    b: &std::collections::BTreeSet<String>,
) -> f64 {
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        return 0.0;
    }
    inter as f64 / union as f64
}

fn detect_repeated_instructions(text: &str, findings: &mut Vec<Finding>) {
    let limit = floor_char_boundary(text, text.len().min(SYSTEM_ZONE_BYTES));
    let sents: Vec<Span> = sentences(&text[..limit])
        .into_iter()
        .filter(|s| s.len() >= MIN_SENTENCE_BYTES)
        .collect();
    if sents.len() < 2 {
        return;
    }

    let sets: Vec<std::collections::BTreeSet<String>> = sents
        .iter()
        .map(|s| word_set(&text[s.start..s.end]))
        .collect();
    let mut parent: Vec<usize> = (0..sents.len()).collect();
    for i in 0..sents.len() {
        for j in (i + 1)..sents.len() {
            if exact_jaccard(&sets[i], &sets[j]) >= SENTENCE_JACCARD {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..sents.len() {
        let root = uf_find(&mut parent, i);
        clusters.entry(root).or_default().push(i);
    }

    for members in clusters.values().filter(|m| m.len() >= 2) {
        let extra_tokens: u64 = members[1..]
            .iter()
            .map(|&m| approx_tokens(sents[m].len()))
            .sum();
        if extra_tokens == 0 {
            continue;
        }
        let rep = &sents[members[1]];
        findings.push(Finding {
            rule: "repeated-instructions".to_string(),
            severity: "medium".to_string(),
            title: "Repeated instructions".to_string(),
            detail: format!(
                "The same instruction appears {} times in the first 4KB (word-set Jaccard >= {SENTENCE_JACCARD}). Models do not need an instruction restated; keep the clearest copy.",
                members.len()
            ),
            byte_start: rep.start,
            byte_end: rep.end,
            savings_min: extra_tokens / 2,
            savings_max: extra_tokens,
            confidence: 0.6,
            badge: BADGE_EXPERIMENTAL.to_string(),
            citation: CITE_REPEATED_INSTRUCTIONS.to_string(),
            preview: None,
        });
    }
}

// ---------------------------------------------------------------------------
// verbose-role-description (experimental)
// ---------------------------------------------------------------------------

fn role_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:you are|your role|as an? |you should always|you must)")
            .expect("static role regex")
    })
}

fn detect_verbose_role_description(text: &str, findings: &mut Vec<Finding>) {
    let limit = floor_char_boundary(text, text.len().min(ROLE_ZONE_BYTES));
    let sents = sentences(&text[..limit]);

    // Longest run of consecutive role-pattern sentences.
    let mut best: Option<(usize, usize)> = None; // (first index, run length)
    let mut run_start = 0usize;
    let mut run_len = 0usize;
    for (i, s) in sents.iter().enumerate() {
        if role_regex().is_match(&text[s.start..s.end]) {
            if run_len == 0 {
                run_start = i;
            }
            run_len += 1;
            if best.is_none_or(|(_, len)| run_len > len) {
                best = Some((run_start, run_len));
            }
        } else {
            run_len = 0;
        }
    }
    let Some((first, len)) = best else { return };
    if len <= MAX_ROLE_SENTENCES {
        return;
    }

    let start = sents[first].start;
    let end = sents[first + len - 1].end;
    let region_tokens = approx_tokens(end - start);
    findings.push(Finding {
        rule: "verbose-role-description".to_string(),
        severity: "low".to_string(),
        title: "Verbose role description".to_string(),
        detail: format!(
            "{len} consecutive sentences describe the assistant's role or persona. One or two clear role sentences carry the same behavioral signal; the rest is repetition tax on every call."
        ),
        byte_start: start,
        byte_end: end,
        savings_min: region_tokens * 5 / 100,
        savings_max: region_tokens * 15 / 100,
        confidence: 0.45,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_ROLE.to_string(),
        preview: None,
    });
}

// ---------------------------------------------------------------------------
// excessive-few-shot (experimental)
// ---------------------------------------------------------------------------

/// Lines that BEGIN an example block. "output"/"a" lines continue a block
/// started by "input"/"q", so they are not block starters.
fn example_start_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^\s*(?:input|example|q)\s*[:#]|<example>").expect("static example regex")
    })
}

/// Any example-marker line, used to find where the last block's content ends.
fn example_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^\s*(?:input|output|example|q|a)\s*[:#]|</?example>")
            .expect("static example regex")
    })
}

fn detect_excessive_few_shot(text: &str, findings: &mut Vec<Finding>) {
    let starts: Vec<usize> = example_start_regex()
        .find_iter(text)
        .map(|m| m.start())
        .collect();
    if starts.len() <= MAX_EXAMPLE_BLOCKS {
        return;
    }

    // The surplus region runs from the start of block MAX+1 to the end of the
    // line holding the last marker (so the final Output:/A: line is included).
    let last_marker = example_marker_regex()
        .find_iter(text)
        .last()
        .expect("starts is non-empty so markers match too");
    let end = text[last_marker.end()..]
        .find('\n')
        .map_or(text.len(), |i| last_marker.end() + i);
    let surplus_start = starts[MAX_EXAMPLE_BLOCKS];
    if end <= surplus_start {
        return;
    }
    let surplus_tokens = approx_tokens(end - surplus_start);
    findings.push(Finding {
        rule: "excessive-few-shot".to_string(),
        severity: "medium".to_string(),
        title: "Excessive few-shot examples".to_string(),
        detail: format!(
            "{} example blocks; accuracy gains plateau after about {MAX_EXAMPLE_BLOCKS} examples, so the {} blocks past that (roughly {surplus_tokens} tokens) usually cost more than they help.",
            starts.len(),
            starts.len() - MAX_EXAMPLE_BLOCKS
        ),
        byte_start: surplus_start,
        byte_end: end,
        savings_min: surplus_tokens / 2,
        savings_max: surplus_tokens,
        confidence: 0.55,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_FEW_SHOT.to_string(),
        preview: None,
    });
}

// ---------------------------------------------------------------------------
// markdown-overhead (experimental)
// ---------------------------------------------------------------------------

fn link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("static link regex"))
}

/// Bytes spent on Markdown markup rather than content: heading markers, list
/// dashes, emphasis asterisks, table pipes, and link syntax (everything in a
/// link except its visible text).
fn markdown_markup_bytes(text: &str) -> usize {
    let mut markup = 0usize;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            let hashes = t.bytes().take_while(|&b| b == b'#').count();
            markup += hashes + 1;
        }
        if t.starts_with("- ") || t.starts_with("+ ") {
            markup += 2;
        }
        markup += line.bytes().filter(|&b| b == b'|' || b == b'*').count();
    }
    for caps in link_regex().captures_iter(text) {
        let full = caps.get(0).expect("group 0 always present");
        markup += full.len() - caps[1].len();
    }
    markup
}

fn detect_markdown_overhead(text: &str, total_tokens: u64, findings: &mut Vec<Finding>) {
    // Fenced code blocks mean the markdown structure is load-bearing.
    if text.contains("```") {
        return;
    }
    let markup = markdown_markup_bytes(text);
    if markup * 100 <= text.len() * MARKDOWN_OVERHEAD_PCT {
        return;
    }
    findings.push(Finding {
        rule: "markdown-overhead".to_string(),
        severity: "low".to_string(),
        title: "Heavy Markdown markup".to_string(),
        detail: format!(
            "Markdown markup is {}% of the input bytes and there are no fenced code blocks. Models read plain text fine; dropping decorative headings, emphasis, and table framing may be cheaper.",
            markup * 100 / text.len()
        ),
        byte_start: 0,
        byte_end: text.len(),
        savings_min: total_tokens * 5 / 100,
        savings_max: total_tokens * 20 / 100,
        confidence: 0.4,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_JSON.to_string(),
        preview: None,
    });
}

// ---------------------------------------------------------------------------
// lost-in-the-middle (experimental)
// ---------------------------------------------------------------------------

fn detect_lost_in_the_middle(text: &str, total_tokens: u64, findings: &mut Vec<Finding>) {
    if total_tokens <= LITM_MIN_TOKENS {
        return;
    }
    let start = floor_char_boundary(text, text.len() * 30 / 100);
    let end = floor_char_boundary(text, text.len() * 70 / 100);
    findings.push(Finding {
        rule: "lost-in-the-middle".to_string(),
        severity: "low".to_string(),
        title: "Low-attention middle zone".to_string(),
        detail: format!(
            "At about {total_tokens} tokens, content in the middle 30-70% of the context gets measurably less attention. Move critical instructions and documents toward the start or end. This is an accuracy finding, not a savings one, so its savings range is intentionally zero."
        ),
        byte_start: start,
        byte_end: end,
        savings_min: 0,
        savings_max: 0,
        confidence: 0.4,
        badge: BADGE_EXPERIMENTAL.to_string(),
        citation: CITE_LITM.to_string(),
        preview: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> AuditReport {
        audit(text, &AuditOptions::default())
    }

    fn rules(report: &AuditReport) -> Vec<&str> {
        report.findings.iter().map(|f| f.rule.as_str()).collect()
    }

    #[test]
    fn empty_text_returns_no_findings() {
        let r = run("");
        assert!(r.findings.is_empty());
        assert_eq!(r.total_savings_min, 0);
        assert_eq!(r.total_savings_max, 0);
    }

    #[test]
    fn near_duplicate_paragraphs_detected() {
        let para = "the quick brown fox jumps over the lazy dog and keeps going through the long meadow toward the river crossing ".repeat(3);
        let text = format!("{para}\n\n{para}");
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "near-duplicate-paragraphs")
            .expect("duplicate paragraphs should be flagged");
        assert!(f.savings_max > 0);
        assert!(f.savings_min <= f.savings_max);
        assert_eq!(f.severity, "medium"); // well under the 500-token high bar
        assert!(f.detail.contains("2 paragraphs"));
        // representative span is the second copy
        assert!(f.byte_start > para.len());
    }

    #[test]
    fn large_duplicate_cluster_is_high_severity() {
        let para = "a long block of context that gets pasted again and again across the conversation, with enough body to clear the duplicate-cluster severity bar once the copies add up ".repeat(16);
        let text = format!("{para}\n\n{para}\n\n{para}");
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "near-duplicate-paragraphs")
            .unwrap();
        assert!(f.savings_max > 500, "{}", f.savings_max);
        assert_eq!(f.severity, "high");
        assert!(f.detail.contains("3 paragraphs"));
    }

    #[test]
    fn distinct_paragraphs_not_flagged() {
        let a = "rust ownership semantics are enforced at compile time by the borrow checker which tracks moves lifetimes and aliasing rules across every function boundary in the program without runtime cost ".repeat(2);
        let b = "the harbor at dawn smelled of salt and diesel while gulls wheeled over the fishing boats returning from a long night on the cold grey water beyond the breakwall and the lighthouse ".repeat(2);
        let r = run(&format!("{a}\n\n{b}"));
        assert!(!rules(&r).contains(&"near-duplicate-paragraphs"));
    }

    #[test]
    fn pretty_json_detected_with_exact_savings() {
        let mut map = serde_json::Map::new();
        for i in 0..20 {
            map.insert(
                format!("configuration_key_number_{i}"),
                serde_json::json!({ "value": i, "enabled": true }),
            );
        }
        let pretty = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap();
        assert!(pretty.len() > MIN_JSON_BYTES);
        let r = run(&pretty);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "json-verbosity")
            .expect("pretty JSON should be flagged");
        assert_eq!(f.savings_min, f.savings_max); // measured exactly by minifying
        assert!(f.savings_max > 0);
    }

    #[test]
    fn minified_json_not_flagged() {
        let mut map = serde_json::Map::new();
        for i in 0..30 {
            map.insert(format!("key_number_{i}"), serde_json::json!(i));
        }
        let compact = serde_json::to_string(&serde_json::Value::Object(map)).unwrap();
        assert!(compact.len() > MIN_JSON_BYTES);
        let r = run(&compact);
        assert!(!rules(&r).contains(&"json-verbosity"));
    }

    #[test]
    fn long_stack_trace_detected() {
        let mut text = String::from("Error: connection refused\n");
        for i in 0..9 {
            text.push_str(&format!(
                "    at handler{i} (src/server/middleware/layer{i}.js:{i}1:1{i})\n"
            ));
        }
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "stack-trace-verbosity")
            .expect("9-frame trace should be flagged");
        assert_eq!(f.severity, "high");
        assert!(f.detail.contains("9 frames"));
        assert!(f.savings_min <= f.savings_max);
        assert!(f.savings_max > 0);
    }

    #[test]
    fn short_stack_trace_not_flagged() {
        let mut text = String::from("Error: boom\n");
        for i in 0..4 {
            text.push_str(&format!("    at handler{i} (src/app.js:{i}:1)\n"));
        }
        let r = run(&text);
        assert!(!rules(&r).contains(&"stack-trace-verbosity"));
    }

    #[test]
    fn volatile_prefix_timestamp_detected() {
        let text = format!(
            "2026-06-09T14:33 deploy log for run\n{}",
            "stable instructions that repeat on every call ".repeat(20)
        );
        let total = approx_tokens(text.len());
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "volatile-prefix")
            .expect("leading timestamp should be flagged");
        assert_eq!(f.savings_min, 0);
        assert_eq!(f.savings_max, total * 90 / 100);
        assert_eq!(f.severity, "medium");
        assert!(f.detail.contains("ISO timestamp"));
        assert_eq!(f.byte_start, 0);
    }

    #[test]
    fn stable_prefix_not_flagged() {
        let text = "stable system instructions with no timestamps, ids, or session noise anywhere near the top of the prompt body at all ".repeat(4);
        let r = run(&text);
        assert!(!rules(&r).contains(&"volatile-prefix"));
    }

    #[test]
    fn sub_cache_threshold_fires_on_reused_prompt() {
        // ~1530 bytes is ~383 tokens: inside the 256..1024 band.
        let text = format!("You are a helpful assistant. {}", "word ".repeat(300));
        let total = approx_tokens(text.len());
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "sub-cache-threshold")
            .expect("'you are' opener inside the band should fire");
        assert_eq!(f.severity, "low");
        assert_eq!(f.savings_min, 0);
        assert_eq!(f.savings_max, total / 2);
    }

    #[test]
    fn sub_cache_threshold_needs_reuse_signal_and_band() {
        // Same size, no "you are", no caller token count: no reuse signal.
        let text = "word ".repeat(300);
        let r = run(&text);
        assert!(!rules(&r).contains(&"sub-cache-threshold"));
        // Reuse signal present but below the band: too small to matter.
        let small = audit("You are terse.", &AuditOptions::default());
        assert!(!rules(&small).contains(&"sub-cache-threshold"));
        // Caller-supplied count above the band: already cacheable.
        let big = audit(
            &"word ".repeat(300),
            &AuditOptions {
                input_tokens: Some(2048),
                ..AuditOptions::default()
            },
        );
        assert!(!rules(&big).contains(&"sub-cache-threshold"));
    }

    #[test]
    fn html_content_detected() {
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!(
                "<div class=\"item\"><span>item {i}</span></div>\n"
            ));
        }
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "html-content")
            .expect("dense tag soup should be flagged");
        assert_eq!(f.severity, "medium"); // region well under 2000 tokens
        assert!(f.savings_min < f.savings_max);
        assert!(f.detail.contains("120 HTML tags"));
    }

    #[test]
    fn sparse_tags_not_flagged() {
        let text = format!(
            "prose mentioning <code> and </code> a few times {}",
            "with plenty of plain text padding between occurrences ".repeat(10)
        );
        let r = run(&text);
        assert!(!rules(&r).contains(&"html-content"));
    }

    #[test]
    fn findings_sorted_by_severity_then_savings() {
        // A high finding (stack trace) plus a medium one (pretty JSON).
        let mut map = serde_json::Map::new();
        for i in 0..20 {
            map.insert(format!("configuration_key_{i}"), serde_json::json!(i));
        }
        let pretty = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap();
        let mut trace = String::from("Error: boom\n");
        for i in 0..8 {
            trace.push_str(&format!(
                "    at handler{i} (src/server/route/handler{i}.js:{i}0:2{i})\n"
            ));
        }
        let r = run(&format!("{pretty}\n\n{trace}"));
        assert!(r.findings.len() >= 2);
        assert_eq!(r.findings[0].severity, "high");
        for pair in r.findings.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (ra, rb) = (severity_rank(&a.severity), severity_rank(&b.severity));
            assert!(ra < rb || (ra == rb && a.savings_max >= b.savings_max));
        }
        assert_eq!(
            r.total_savings_max,
            r.findings.iter().map(|f| f.savings_max).sum::<u64>()
        );
    }

    #[test]
    fn notes_label_estimates_and_approximation() {
        let approx = run("hello there");
        assert!(approx
            .notes
            .contains(&"Savings are input-token estimates; output may vary.".to_string()));
        assert!(approx
            .notes
            .contains(&"Token figures use a bytes/4 approximation.".to_string()));

        let exact = audit(
            "hello there",
            &AuditOptions {
                input_tokens: Some(3),
                ..AuditOptions::default()
            },
        );
        assert!(exact
            .notes
            .contains(&"Savings are input-token estimates; output may vary.".to_string()));
        assert!(!exact
            .notes
            .iter()
            .any(|n| n.contains("bytes/4 approximation")));
    }

    #[test]
    fn findings_have_badge_and_citation_and_serialize() {
        let text = format!(
            "2026-06-09T14:33 boot\n{}",
            "stable text that pads the prompt out ".repeat(10)
        );
        let r = run(&text);
        assert!(!r.findings.is_empty());
        for f in &r.findings {
            assert_eq!(f.badge, "production-proven");
            assert!(f.citation.starts_with("https://"));
            assert!((0.0..=1.0).contains(&f.confidence));
        }
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"total_savings_min\""));
        let back: AuditReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.findings.len(), r.findings.len());
    }

    // -- Phase 3: experimental rules and format previews ---------------------

    fn run_exp(text: &str) -> AuditReport {
        audit(
            text,
            &AuditOptions {
                include_experimental: true,
                ..AuditOptions::default()
            },
        )
    }

    #[test]
    fn audit_options_include_experimental_round_trips_through_json() {
        // The WASM binding passes options as a JSON string; this is the shape
        // it must accept.
        let on: AuditOptions = serde_json::from_str(r#"{"include_experimental":true}"#).unwrap();
        assert!(on.include_experimental);
        assert!(on.input_tokens.is_none());
        let off: AuditOptions = serde_json::from_str("{}").unwrap();
        assert!(!off.include_experimental);
    }

    #[test]
    fn filler_phrases_detected_at_threshold() {
        let text = "Please note that you should basically keep it short. It is important to actually test things. In order to proceed, really focus on the goal.";
        let r = run_exp(text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "filler-phrases")
            .expect("6 filler hits should fire");
        assert_eq!(f.severity, "low");
        assert_eq!(f.badge, "experimental");
        assert!(f.detail.contains("6 filler"));
        let total = approx_tokens(text.len());
        assert_eq!(f.savings_min, total * 5 / 100);
        assert_eq!(f.savings_max, total * 15 / 100);
    }

    #[test]
    fn filler_words_only_count_standalone() {
        // "justify", "adjusting", "everything" contain filler words as
        // substrings but must not count; the 4 standalone hits stay below the
        // 5-hit threshold.
        let text =
            "Justify adjusting everything reverently. Be very direct, really just actually direct.";
        let r = run_exp(text);
        assert!(!rules(&r).contains(&"filler-phrases"));
    }

    #[test]
    fn repeated_instructions_detected_in_system_zone() {
        let inst = "Always respond in valid JSON format with no extra commentary whatsoever";
        let text = format!("{inst}.\nProcess the user request promptly.\n{inst}.\n");
        let r = run_exp(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "repeated-instructions")
            .expect("a repeated sentence should fire");
        assert_eq!(f.severity, "medium");
        assert_eq!(f.badge, "experimental");
        assert!(f.detail.contains("2 times"));
        assert!(f.savings_max >= f.savings_min);
        assert!(f.savings_max > 0);
        // Representative span is the second copy.
        assert!(f.byte_start > inst.len());
    }

    #[test]
    fn distinct_instructions_not_flagged() {
        let text = "Always respond in valid JSON format with no extra commentary whatsoever.\nSummarize each document in exactly three bullet points for the reviewer.\n";
        let r = run_exp(text);
        assert!(!rules(&r).contains(&"repeated-instructions"));
    }

    #[test]
    fn verbose_role_description_detected() {
        let text = "You are a meticulous senior engineer with deep expertise. You must always reason step by step before answering. Your role is to review code for correctness and safety. You should always keep responses brief and direct. The repository uses a monorepo layout.";
        let r = run_exp(text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "verbose-role-description")
            .expect("4 consecutive role sentences should fire");
        assert_eq!(f.severity, "low");
        assert_eq!(f.badge, "experimental");
        assert!(f.detail.contains("4 consecutive"));
        assert!(f.savings_min < f.savings_max);
        assert_eq!(f.byte_start, 0);
    }

    #[test]
    fn short_role_description_not_flagged() {
        // Three role sentences sit at the tolerance bar.
        let text = "You are a senior engineer. You must reason carefully. Your role is code review. The repository uses a monorepo layout with two apps.";
        let r = run_exp(text);
        assert!(!rules(&r).contains(&"verbose-role-description"));
    }

    #[test]
    fn excessive_few_shot_detected() {
        let mut text = String::from("Classify the sentiment of each line.\n");
        for i in 0..7 {
            text.push_str(&format!("Input: sample sentence number {i}\n"));
            text.push_str(&format!("Output: label_{i}\n"));
        }
        let r = run_exp(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "excessive-few-shot")
            .expect("7 example blocks should fire");
        assert_eq!(f.severity, "medium");
        assert_eq!(f.badge, "experimental");
        assert!(f.detail.contains("7 example blocks"));
        assert!(f.savings_max > 0);
        // The surplus region starts at the 6th Input: line and includes the
        // final Output: line.
        assert_eq!(
            f.byte_start,
            text.find("Input: sample sentence number 5").unwrap()
        );
        assert_eq!(f.byte_end, text.trim_end().len());
    }

    #[test]
    fn five_examples_not_flagged() {
        let mut text = String::from("Classify the sentiment of each line.\n");
        for i in 0..5 {
            text.push_str(&format!("Input: sample sentence number {i}\n"));
            text.push_str(&format!("Output: label_{i}\n"));
        }
        let r = run_exp(&text);
        assert!(!rules(&r).contains(&"excessive-few-shot"));
    }

    #[test]
    fn markdown_overhead_detected_without_code_fences() {
        let text =
            "## Heading\n**bold** *text* | cell | cell |\n- item one\n- item two\n".repeat(4);
        let r = run_exp(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "markdown-overhead")
            .expect("markup-heavy text should fire");
        assert_eq!(f.severity, "low");
        assert_eq!(f.badge, "experimental");
        assert_eq!(f.byte_end, text.len());
        assert!(f.savings_min < f.savings_max);

        // The same text with a fenced code block is structural, not flagged.
        let fenced = format!("```\ncode\n```\n{text}");
        let r2 = run_exp(&fenced);
        assert!(!rules(&r2).contains(&"markdown-overhead"));
    }

    #[test]
    fn lost_in_the_middle_fires_on_long_contexts_with_zero_savings() {
        let text = "word ".repeat(200);
        let r = audit(
            &text,
            &AuditOptions {
                input_tokens: Some(60_000),
                include_experimental: true,
            },
        );
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "lost-in-the-middle")
            .expect("60K tokens should fire");
        assert_eq!(f.savings_min, 0);
        assert_eq!(f.savings_max, 0);
        assert_eq!(f.badge, "experimental");
        assert!(f.detail.contains("accuracy finding, not a savings one"));
        assert_eq!(f.byte_start, text.len() * 30 / 100);
        assert_eq!(f.byte_end, text.len() * 70 / 100);

        // Same flag, short context: no finding.
        let short = run_exp(&text);
        assert!(!rules(&short).contains(&"lost-in-the-middle"));
    }

    #[test]
    fn experimental_rules_gated_off_by_default() {
        // Text that trips several experimental rules under the flag.
        let text = "You are an expert. You are an expert. Please note that it is important to basically just really do the thing, actually.";
        let with_flag = run_exp(text);
        assert!(with_flag.findings.iter().any(|f| f.badge == "experimental"));

        let without = run(text);
        assert!(!without.findings.iter().any(|f| f.badge == "experimental"));
        assert!(!without.notes.iter().any(|n| n.contains("false-positive")));
    }

    #[test]
    fn experimental_note_added_only_when_experimental_findings_exist() {
        let text = "Please note that you should basically keep it short. It is important to actually test things. In order to proceed, really focus on the goal.";
        let r = run_exp(text);
        assert!(r.notes.contains(&EXPERIMENTAL_NOTE.to_string()));

        // Flag on, nothing experimental fired: no note.
        let quiet = run_exp("a plain short sentence with no waste patterns");
        assert!(!quiet.findings.iter().any(|f| f.badge == "experimental"));
        assert!(!quiet.notes.contains(&EXPERIMENTAL_NOTE.to_string()));
    }

    #[test]
    fn json_verbosity_carries_minify_preview() {
        let mut map = serde_json::Map::new();
        for i in 0..20 {
            map.insert(
                format!("configuration_key_number_{i}"),
                serde_json::json!({ "value": i, "enabled": true }),
            );
        }
        let pretty = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap();
        let r = run(&pretty);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "json-verbosity")
            .unwrap();
        let p = f.preview.as_ref().expect("strict JSON gets a preview");
        assert_eq!(p.kind, "json-minify");
        assert_eq!(p.fidelity, "lossless");
        assert!(p.bytes_after < p.bytes_before);

        // Findings without a preview serialize without the key at all.
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"preview\""));
        let no_preview = run("2026-06-09T14:33 boot\nstable text that pads the prompt body out to a useful length for the volatile rule");
        assert!(!no_preview.findings.is_empty());
        let json2 = serde_json::to_string(&no_preview).unwrap();
        assert!(!json2.contains("\"preview\""));
    }

    #[test]
    fn json_toon_candidate_emitted_only_under_experimental() {
        let rows: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({ "id": i, "name": format!("item-{i}"), "active": true }))
            .collect();
        let pretty = serde_json::to_string_pretty(&serde_json::Value::Array(rows)).unwrap();
        assert!(pretty.len() > MIN_JSON_BYTES);

        let r = run_exp(&pretty);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "json-toon-candidate")
            .expect("uniform array should produce a TOON candidate");
        assert_eq!(f.badge, "experimental");
        assert_eq!(f.citation, CITE_TOON);
        let p = f
            .preview
            .as_ref()
            .expect("TOON candidate carries a preview");
        assert_eq!(p.kind, "json-toon");
        assert!(p.preview.starts_with("items[10]{active,id,name}:"));
        assert!(f.savings_max > 0);
        assert!(rules(&r).contains(&"json-verbosity"));

        let without = run(&pretty);
        assert!(!rules(&without).contains(&"json-toon-candidate"));
    }

    #[test]
    fn html_content_carries_markdown_preview_with_measured_savings() {
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!(
                "<div class=\"item\"><span>item {i}</span></div>\n"
            ));
        }
        let r = run(&text);
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == "html-content")
            .unwrap();
        let p = f.preview.as_ref().expect("converted HTML gets a preview");
        assert_eq!(p.kind, "html-markdown");
        assert!(p.bytes_after < p.bytes_before);
        let measured = approx_tokens(p.bytes_before - p.bytes_after);
        assert_eq!(f.savings_max, measured);
        assert_eq!(f.savings_min, measured * 60 / 100);
        assert!(f.detail.contains("measured by converting"));
        assert!(p.preview.contains("item 0"));
        assert!(!p.preview.contains("<div"));
    }
}
