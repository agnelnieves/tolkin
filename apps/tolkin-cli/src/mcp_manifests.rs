//! Per-project committable cache of MCP tools/list manifests.
//!
//! When `tolkin mcp --probe <server>` succeeds, the captured tools/list is
//! redacted, pinned to a stable shape, and written under
//! `<project root>/.tolkin/mcp-manifests/<slug>.json`. Teammates who run
//! `tolkin mcp` without the probe flag pick the cached file up automatically,
//! so exact per-tool counts travel with the repo and nobody has to re-probe.
//!
//! Shape (pinned at v=1; do not break this without a v=2 migration):
//! ```json
//! {
//!   "v": 1,
//!   "server": "tracker",
//!   "captured_at": "2026-06-11",
//!   "transport": "stdio" | "http",
//!   "source": "probe" | "file",
//!   "protocol_version": "2025-03-26" | null,
//!   "tools": [ ...raw tool objects from the server... ]
//! }
//! ```
//!
//! Tool objects are stored verbatim from the server (after redaction), not
//! the canonical compact serialization. Tokenization happens at read time
//! through `tolkin_core::mcp_tools::parse_tools_list`, so a cached file is
//! always re-tokenized with whichever tokenizer the user picks today.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tolkin_core::redact::{self, RedactOptions};

/// On-disk schema version. Bumped only on a breaking change to the shape.
pub const SCHEMA_VERSION: u32 = 1;

/// How old a manifest can be before the analyzer prints a staleness warning.
pub const STALE_AFTER_DAYS: i64 = 90;

/// Directory the analyzer reads cached manifests from, relative to the
/// project root.
pub const CACHE_DIR_REL: &str = ".tolkin/mcp-manifests";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub server: String,
    /// YYYY-MM-DD in UTC. String, not a date type, so the file diff is a
    /// human-readable single field, not a chrono blob.
    pub captured_at: String,
    /// Transport that produced this manifest: "stdio" or "http".
    pub transport: String,
    /// "probe" (captured live by tolkin) or "file" (imported from a file).
    pub source: String,
    /// Server-advertised MCP protocol version, when the initialize response
    /// carried one. Optional so HTTP servers that skip strict init still cache.
    pub protocol_version: Option<String>,
    /// Raw tool objects exactly as the server returned them, after redaction
    /// of secrets in descriptions and other string fields.
    pub tools: Vec<Value>,
}

/// Lowercase, non-alphanumeric to '-', collapse runs, trim hyphens. The
/// transform is pure so it can be unit-tested without touching disk.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_hyphen = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_hyphen = false;
        } else if !last_hyphen {
            out.push('-');
            last_hyphen = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed
    }
}

/// Today as YYYY-MM-DD UTC. Pure conversion of seconds-since-epoch to a
/// civil date, no chrono dependency.
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    civil_date_from_unix(secs)
}

/// Civil date YYYY-MM-DD from a Unix timestamp in seconds (UTC). Uses
/// Howard Hinnant's days-from-civil algorithm, exact for any year >= 0.
pub fn civil_date_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Inverse of days_from_civil; returns (year, month, day_of_month). The
/// algorithm is well-known; see Hinnant 2013.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64; // 0..=146096
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // 0..=399
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365
    let mp = (5 * doy + 2) / 153; // 0..=11
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Inverse of civil_from_days: returns days since 1970-01-01 for a UTC date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // 0..=399
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // 0..=365
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // 0..=146096
    era * 146_097 + doe - 719_468
}

/// Days between `captured_at` (YYYY-MM-DD) and `today` (YYYY-MM-DD). Returns
/// None when either date does not parse; positive when today is later.
pub fn days_between(captured_at: &str, today: &str) -> Option<i64> {
    let (cy, cm, cd) = parse_ymd(captured_at)?;
    let (ty, tm, td) = parse_ymd(today)?;
    Some(days_from_civil(ty, tm, td) - days_from_civil(cy, cm, cd))
}

fn parse_ymd(s: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// Staleness check: returns a warning string when `manifest.captured_at` is
/// older than [`STALE_AFTER_DAYS`] days from `today`, else None.
pub fn staleness(manifest: &Manifest, today: &str) -> Option<String> {
    let age = days_between(&manifest.captured_at, today)?;
    if age > STALE_AFTER_DAYS {
        Some(format!(
            "manifest is {age} days old (>{STALE_AFTER_DAYS}); re-probe to refresh"
        ))
    } else {
        None
    }
}

/// Absolute path to the cache directory for a project root.
pub fn cache_dir(root: &Path) -> PathBuf {
    root.join(CACHE_DIR_REL)
}

/// Absolute path to the cache file for one server in a project root.
pub fn manifest_path(root: &Path, server: &str) -> PathBuf {
    cache_dir(root).join(format!("{}.json", slugify(server)))
}

/// Build a Manifest from raw tool objects, slotting in today's date and
/// running the secret-redaction pass over the JSON text before deserializing
/// back. The redaction round-trips through JSON so secrets embedded inside
/// nested fields are scrubbed wherever they live.
pub fn build_manifest(
    server: &str,
    transport: &str,
    source: &str,
    protocol_version: Option<String>,
    tools: Vec<Value>,
) -> Result<Manifest> {
    let raw = Manifest {
        v: SCHEMA_VERSION,
        server: server.to_string(),
        captured_at: today_utc(),
        transport: transport.to_string(),
        source: source.to_string(),
        protocol_version,
        tools,
    };
    let text = serde_json::to_string(&raw).context("serialize manifest for redaction")?;
    let redacted = redact::redact(&text, &RedactOptions::default()).redacted_text;
    let manifest: Manifest =
        serde_json::from_str(&redacted).context("reparse redacted manifest")?;
    Ok(manifest)
}

/// Persist a manifest under the project root. Writes pretty JSON with a
/// trailing newline so a `git diff` reads cleanly. Always rewrites the file
/// (atomic enough for the cache: a partial write would fail to parse and the
/// next probe would replace it).
pub fn save_manifest(root: &Path, manifest: &Manifest) -> Result<PathBuf> {
    let dir = cache_dir(root);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = manifest_path(root, &manifest.server);
    let mut text = serde_json::to_string_pretty(manifest).context("serialize manifest")?;
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Load the cached manifest for one server, when present and parseable.
/// A parse error returns None (not an error): the cache is best-effort, and
/// a corrupt file should not crash the analyzer.
pub fn load_for(root: &Path, server: &str) -> Option<Manifest> {
    let path = manifest_path(root, server);
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Load every cached manifest under the project root, in slug order.
#[allow(dead_code)] // public surface for future stats/diagnostics; tested below.
pub fn load_all(root: &Path) -> Vec<Manifest> {
    let dir = cache_dir(root);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(m) = serde_json::from_str::<Manifest>(&text) {
                out.push(m);
            }
        }
    }
    out
}

/// Build the equivalent of an MCP `{"tools": [...]}` payload from a cached
/// manifest, suitable for handing straight to
/// `tolkin_core::mcp_tools::parse_tools_list`.
pub fn to_tools_list_text(manifest: &Manifest) -> String {
    serde_json::to_string(&serde_json::json!({ "tools": &manifest.tools }))
        .unwrap_or_else(|_| "{\"tools\":[]}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_replaces_non_alnum() {
        assert_eq!(slugify("Tracker"), "tracker");
        assert_eq!(slugify("My Server v2"), "my-server-v2");
        assert_eq!(slugify("a/b\\c.d"), "a-b-c-d");
        assert_eq!(slugify("__weird__name__"), "weird-name");
        assert_eq!(slugify("trim---"), "trim");
        assert_eq!(slugify("---"), "server");
        assert_eq!(slugify(""), "server");
    }

    #[test]
    fn civil_date_round_trip() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 6, 11), (2100, 12, 31)] {
            let days = days_from_civil(y, m, d);
            let (y2, m2, d2) = civil_from_days(days);
            assert_eq!((y, m, d), (y2, m2, d2));
        }
    }

    #[test]
    fn civil_date_from_unix_known_values() {
        // 1970-01-01 00:00 UTC.
        assert_eq!(civil_date_from_unix(0), "1970-01-01");
        // 2000-01-01 00:00 UTC = 946_684_800.
        assert_eq!(civil_date_from_unix(946_684_800), "2000-01-01");
        // 2026-06-11 00:00 UTC = 1_781_136_000.
        assert_eq!(civil_date_from_unix(1_781_136_000), "2026-06-11");
    }

    #[test]
    fn days_between_handles_dates() {
        assert_eq!(days_between("2026-06-11", "2026-06-11"), Some(0));
        assert_eq!(days_between("2026-01-01", "2026-01-02"), Some(1));
        assert_eq!(days_between("2026-01-01", "2026-12-31"), Some(364));
        assert_eq!(days_between("bad", "2026-01-01"), None);
    }

    #[test]
    fn staleness_fires_after_threshold() {
        let m = Manifest {
            v: 1,
            server: "x".to_string(),
            captured_at: "2026-01-01".to_string(),
            transport: "stdio".to_string(),
            source: "probe".to_string(),
            protocol_version: None,
            tools: Vec::new(),
        };
        assert!(staleness(&m, "2026-02-01").is_none());
        assert!(staleness(&m, "2026-04-01").is_none()); // exactly 90 days
        assert!(staleness(&m, "2026-05-01").is_some());
    }

    #[test]
    fn manifest_round_trip_redacts_secrets() {
        let tools = vec![serde_json::json!({
            "name": "create_token",
            "description": "Creates a token. Example: sk-test-1234567890abcdef0123",
            "inputSchema": { "type": "object" }
        })];
        let m = build_manifest("tracker", "stdio", "probe", None, tools).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "tolkin-manifest-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = save_manifest(&dir, &m).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("sk-test-1234567890abcdef"),
            "redaction must scrub the planted secret: {text}"
        );
        assert!(text.contains("REDACTED"), "{text}");

        // Load back and the shape is intact.
        let loaded = load_for(&dir, "tracker").expect("cached manifest");
        assert_eq!(loaded.v, 1);
        assert_eq!(loaded.server, "tracker");
        assert_eq!(loaded.transport, "stdio");
        assert_eq!(loaded.source, "probe");
        assert_eq!(loaded.tools.len(), 1);

        let all = load_all(&dir);
        assert_eq!(all.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn to_tools_list_text_wraps_in_tools_object() {
        let m = Manifest {
            v: 1,
            server: "x".to_string(),
            captured_at: today_utc(),
            transport: "stdio".to_string(),
            source: "probe".to_string(),
            protocol_version: None,
            tools: vec![serde_json::json!({ "name": "a" })],
        };
        let text = to_tools_list_text(&m);
        assert!(text.starts_with("{\"tools\":["));
        assert!(text.contains("\"name\":\"a\""));
    }
}
