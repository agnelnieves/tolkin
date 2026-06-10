//! Format-efficiency previews: small, dependency-free converters that show
//! what a span would look like in a cheaper representation. Previews are
//! advisory; the audit attaches them to findings so the user can see the
//! exact transformation before acting. Each preview carries a fidelity label
//! and a caveat because none of these conversions is free of tradeoffs
//! except plain JSON minification.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Previews are display artifacts, not the conversion API, so they get capped
/// to keep reports small. `bytes_after` always reflects the full conversion.
const PREVIEW_CAP_BYTES: usize = 8 * 1024;
const TRUNCATION_SUFFIX: &str = "... truncated";
/// TOON only pays off when the tabular header amortizes over enough rows.
const TOON_MIN_ROWS: usize = 3;

/// One converted-format preview attached to a finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormatPreview {
    /// "json-minify" | "json-toon" | "html-markdown".
    pub kind: String,
    /// The converted text, capped at 8KB with a truncation suffix.
    pub preview: String,
    pub bytes_before: usize,
    pub bytes_after: usize,
    /// "lossless" | "near-lossless" | "lossy-low-risk".
    pub fidelity: String,
    pub caveat: String,
}

fn floor_char_boundary(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn cap_preview(converted: String) -> String {
    if converted.len() <= PREVIEW_CAP_BYTES {
        return converted;
    }
    let cut = floor_char_boundary(&converted, PREVIEW_CAP_BYTES);
    format!("{}{TRUNCATION_SUFFIX}", &converted[..cut])
}

/// Re-serialize a JSON span compact. `None` when the span is not strict JSON.
pub fn json_minify(span: &str) -> Option<FormatPreview> {
    let value: serde_json::Value = serde_json::from_str(span).ok()?;
    let minified = serde_json::to_string(&value).ok()?;
    Some(FormatPreview {
        kind: "json-minify".to_string(),
        bytes_before: span.len(),
        bytes_after: minified.len(),
        preview: cap_preview(minified),
        fidelity: "lossless".to_string(),
        caveat: String::new(),
    })
}

/// True for scalar JSON values TOON can put in a table cell.
fn is_scalar(v: &serde_json::Value) -> bool {
    matches!(
        v,
        serde_json::Value::String(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::Bool(_)
            | serde_json::Value::Null
    )
}

fn toon_cell(v: &serde_json::Value) -> String {
    match v {
        // Commas are the TOON column separator, so values containing one get
        // quoted to keep the row parseable.
        serde_json::Value::String(s) => {
            if s.contains(',') {
                format!("\"{s}\"")
            } else {
                s.clone()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => "null".to_string(),
    }
}

/// Convert a JSON array of uniform flat objects to TOON tabular form. `None`
/// unless the span is an array of >= 3 objects sharing an identical key set
/// with only scalar values; anything else risks costing more than compact
/// JSON (the format's own caveat).
pub fn json_to_toon(span: &str) -> Option<FormatPreview> {
    let value: serde_json::Value = serde_json::from_str(span).ok()?;
    let rows = value.as_array()?;
    if rows.len() < TOON_MIN_ROWS {
        return None;
    }
    let first = rows[0].as_object()?;
    if first.is_empty() {
        return None;
    }
    let keys: Vec<&String> = first.keys().collect();
    for row in rows {
        let obj = row.as_object()?;
        if obj.len() != keys.len() || !keys.iter().all(|k| obj.get(*k).is_some_and(is_scalar)) {
            return None;
        }
    }

    let mut out = format!(
        "items[{}]{{{}}}:\n",
        rows.len(),
        keys.iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    for row in rows {
        let obj = row.as_object().expect("validated above");
        let cells: Vec<String> = keys
            .iter()
            .map(|k| toon_cell(obj.get(*k).expect("validated above")))
            .collect();
        out.push_str("  ");
        out.push_str(&cells.join(","));
        out.push('\n');
    }

    Some(FormatPreview {
        kind: "json-toon".to_string(),
        bytes_before: span.len(),
        bytes_after: out.len(),
        preview: cap_preview(out),
        fidelity: "lossy-low-risk".to_string(),
        caveat: "TOON must be taught to the model by example; deeply nested data can cost more than compact JSON.".to_string(),
    })
}

struct HtmlRegexes {
    script: Regex,
    style: Regex,
    heading_open: Regex,
    heading_close: Regex,
    li_open: Regex,
    li_close: Regex,
    block_boundary: Regex,
    anchor: Regex,
    strong: Regex,
    em: Regex,
    any_tag: Regex,
    excess_newlines: Regex,
}

fn html_regexes() -> &'static HtmlRegexes {
    static RES: OnceLock<HtmlRegexes> = OnceLock::new();
    RES.get_or_init(|| HtmlRegexes {
        script: Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").expect("static html regex"),
        style: Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").expect("static html regex"),
        heading_open: Regex::new(r"(?i)<h([1-6])\b[^>]*>").expect("static html regex"),
        heading_close: Regex::new(r"(?i)</h[1-6]\s*>").expect("static html regex"),
        li_open: Regex::new(r"(?i)<li\b[^>]*>").expect("static html regex"),
        li_close: Regex::new(r"(?i)</li\s*>").expect("static html regex"),
        block_boundary: Regex::new(r"(?i)</?(?:p|div)\b[^>]*>|<br\s*/?>")
            .expect("static html regex"),
        anchor: Regex::new(r#"(?is)<a\b[^>]*\bhref\s*=\s*["']?([^"'\s>]+)["']?[^>]*>(.*?)</a\s*>"#)
            .expect("static html regex"),
        strong: Regex::new(r"(?i)</?(?:strong|b)\b[^>]*>").expect("static html regex"),
        em: Regex::new(r"(?i)</?(?:em|i)\b[^>]*>").expect("static html regex"),
        any_tag: Regex::new(r"(?s)</?[A-Za-z][^>]*>").expect("static html regex"),
        excess_newlines: Regex::new(r"\n{3,}").expect("static html regex"),
    })
}

/// Minimal hand-rolled HTML to Markdown conversion. `None` when the span has
/// no tags to convert. Deliberately not a full parser: the goal is a faithful
/// enough preview of the token cost, not a publishing pipeline.
pub fn html_to_markdown(span: &str) -> Option<FormatPreview> {
    let res = html_regexes();
    if !res.any_tag.is_match(span) {
        return None;
    }

    let text = res.script.replace_all(span, "");
    let text = res.style.replace_all(&text, "");
    let text = res
        .heading_open
        .replace_all(&text, |caps: &regex::Captures| {
            let level: usize = caps[1].parse().expect("regex guarantees 1-6");
            format!("\n\n{} ", "#".repeat(level))
        });
    let text = res.heading_close.replace_all(&text, "\n\n");
    let text = res.li_open.replace_all(&text, "\n- ");
    let text = res.li_close.replace_all(&text, "");
    let text = res.anchor.replace_all(&text, "[$2]($1)");
    let text = res.strong.replace_all(&text, "**");
    let text = res.em.replace_all(&text, "*");
    let text = res.block_boundary.replace_all(&text, "\n");
    let text = res.any_tag.replace_all(&text, "");
    // Decode &amp; last so "&amp;lt;" stays an escaped literal, not a tag.
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let text = res.excess_newlines.replace_all(&text, "\n\n");
    let markdown = text.trim().to_string();

    Some(FormatPreview {
        kind: "html-markdown".to_string(),
        bytes_before: span.len(),
        bytes_after: markdown.len(),
        preview: cap_preview(markdown),
        fidelity: "near-lossless".to_string(),
        caveat: "Attribute data and layout semantics are dropped.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_minify_is_lossless_and_measured() {
        let span = "{\n  \"name\": \"tolkin\",\n  \"count\": 3\n}";
        let p = json_minify(span).expect("valid JSON should minify");
        assert_eq!(p.kind, "json-minify");
        assert_eq!(p.preview, "{\"count\":3,\"name\":\"tolkin\"}");
        assert_eq!(p.bytes_before, span.len());
        assert_eq!(p.bytes_after, p.preview.len());
        assert_eq!(p.fidelity, "lossless");
        assert!(p.caveat.is_empty());
    }

    #[test]
    fn json_minify_rejects_invalid_json() {
        assert!(json_minify("{ not json }").is_none());
        assert!(json_minify("plain prose").is_none());
    }

    #[test]
    fn toon_happy_path_with_comma_escaping() {
        let span = r#"[
            {"id": 1, "name": "alpha", "ok": true},
            {"id": 2, "name": "beta, the second", "ok": false},
            {"id": 3, "name": "gamma", "ok": null}
        ]"#;
        let p = json_to_toon(span).expect("uniform array should convert");
        assert_eq!(p.kind, "json-toon");
        assert_eq!(p.fidelity, "lossy-low-risk");
        assert_eq!(
            p.preview,
            "items[3]{id,name,ok}:\n  1,alpha,true\n  2,\"beta, the second\",false\n  3,gamma,null\n"
        );
        assert!(p.caveat.contains("taught to the model"));
    }

    #[test]
    fn toon_rejects_unqualified_arrays() {
        // Fewer than 3 rows.
        assert!(json_to_toon(r#"[{"a":1},{"a":2}]"#).is_none());
        // Non-uniform key sets.
        assert!(json_to_toon(r#"[{"a":1},{"b":2},{"a":3}]"#).is_none());
        // Nested (non-scalar) values.
        assert!(json_to_toon(r#"[{"a":{"x":1}},{"a":{"x":2}},{"a":{"x":3}}]"#).is_none());
        // Not an array of objects.
        assert!(json_to_toon(r#"[1,2,3]"#).is_none());
        assert!(json_to_toon(r#"{"a":1}"#).is_none());
    }

    #[test]
    fn html_to_markdown_basics() {
        let html = concat!(
            "<html><head><style>body { color: red; }</style>",
            "<script>alert('tracking');</script></head><body>",
            "<h2 class=\"x\">Title &amp; More</h2>",
            "<p>Read <a href=\"https://example.com\">the docs</a> now.</p>",
            "<ul><li><strong>bold</strong> item</li><li><em>soft</em> item</li></ul>",
            "</body></html>"
        );
        let p = html_to_markdown(html).expect("tag soup should convert");
        assert_eq!(p.kind, "html-markdown");
        assert_eq!(p.fidelity, "near-lossless");
        let md = &p.preview;
        assert!(!md.contains("alert"), "{md}");
        assert!(!md.contains("color: red"), "{md}");
        assert!(md.contains("## Title & More"), "{md}");
        assert!(md.contains("[the docs](https://example.com)"), "{md}");
        assert!(md.contains("- **bold** item"), "{md}");
        assert!(md.contains("- *soft* item"), "{md}");
        assert!(!md.contains('<'), "{md}");
        assert!(!md.contains("\n\n\n"), "{md}");
        assert!(p.bytes_after < p.bytes_before);
    }

    #[test]
    fn html_to_markdown_requires_tags() {
        assert!(html_to_markdown("plain prose, no markup at all").is_none());
    }

    #[test]
    fn preview_capped_at_8kb() {
        let row = r#"{"key": "value value value value value value value"},"#;
        let body = row.repeat(400);
        let span = format!("[{}{}]", body, r#"{"key": "tail"}"#);
        let p = json_minify(&span).expect("valid JSON");
        assert!(span.len() > PREVIEW_CAP_BYTES);
        assert!(p.preview.len() <= PREVIEW_CAP_BYTES + TRUNCATION_SUFFIX.len());
        assert!(p.preview.ends_with(TRUNCATION_SUFFIX));
        // bytes_after reflects the full conversion, not the capped preview.
        assert!(p.bytes_after > p.preview.len());
    }
}
