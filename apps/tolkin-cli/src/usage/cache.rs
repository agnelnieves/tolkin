//! Pull-based parse cache for usage-log ingestion.
//!
//! Each log file is keyed by its absolute path; the cache entry stores the
//! file's mtime + size and the parsed result. For Codex the cached result is
//! one SessionUsage per file (a rollout IS a session and they do not share
//! ids across files). For Claude Code the cached result is the per-file stage
//! one output: the within-file last-wins deduped records. Stage two (the
//! global cross-file merge) ALWAYS runs fresh; it is cheap and would be
//! correctness-busting to cache, since one new file can change the winner for
//! a key that lives in another, unchanged file.
//!
//! Privacy posture: the cache stores nothing but token counts, timestamps,
//! cwd strings, raw model ids, and the opaque `message_id` / `requestId`
//! identifiers used for dedup. No message content.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::claude_code::{self, DedupedRecord};
use super::codex;
use super::types::{RequestUsage, SessionUsage, UsageData, UsageSource, UsageTotals};

/// Bump on any breaking change to the on-disk schema. Mismatched versions are
/// treated as a cold cache (no error, just a full reparse). v2 replaced the
/// Claude side's cached SessionUsage with cached stage-one records so global
/// dedup runs correctly across files. v3 marks the per-request retention
/// release (SessionUsage grew a `requests` vector, mirrored below for the
/// Codex payload): any pre-retention cache is discarded wholesale by the
/// version gate instead of reasoning about partial compatibility, so v2
/// caches self-heal by silent full reparse exactly like v1 did.
const CACHE_VERSION: u32 = 3;

const CACHE_FILE: &str = "usage-cache.json";

/// Either the Claude side (stage-one deduped records) or the Codex side
/// (already a session).
#[derive(Clone, Debug)]
enum ParsedPayload {
    Claude { records: Vec<DedupedRecord> },
    Codex { sessions: Vec<SessionUsage> },
}

#[derive(Clone, Debug)]
struct CacheEntry {
    mtime_secs: u64,
    size_bytes: u64,
    skipped_lines: u64,
    /// The source-and-parser this file was last parsed by, so we never feed a
    /// Codex file to the Claude parser on a path collision.
    source: UsageSource,
    payload: ParsedPayload,
}

#[derive(Clone, Debug)]
struct CacheFile {
    v: u32,
    entries: BTreeMap<String, CacheEntry>,
}

impl Default for CacheFile {
    fn default() -> Self {
        Self {
            v: CACHE_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

// types.rs is the frozen contract: SessionUsage and UsageSource are
// Serialize-only. To round-trip the cache we mirror them with Deserialize-
// capable shadows here, then convert field-for-field. UsageSource serializes
// as a lowercase string (snake_case via serde) so the mirror uses a String.

/// types.rs's UsageTotals is Serialize-only; this mirror gives us a
/// Deserialize-able sibling for the on-disk cache. Field-for-field identical.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct UsageTotalsMirror {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_5m_tokens: u64,
    cache_write_1h_tokens: u64,
}

impl From<UsageTotals> for UsageTotalsMirror {
    fn from(t: UsageTotals) -> Self {
        UsageTotalsMirror {
            input_tokens: t.input_tokens,
            output_tokens: t.output_tokens,
            cache_read_tokens: t.cache_read_tokens,
            cache_write_5m_tokens: t.cache_write_5m_tokens,
            cache_write_1h_tokens: t.cache_write_1h_tokens,
        }
    }
}

impl From<UsageTotalsMirror> for UsageTotals {
    fn from(t: UsageTotalsMirror) -> Self {
        UsageTotals {
            input_tokens: t.input_tokens,
            output_tokens: t.output_tokens,
            cache_read_tokens: t.cache_read_tokens,
            cache_write_5m_tokens: t.cache_write_5m_tokens,
            cache_write_1h_tokens: t.cache_write_1h_tokens,
        }
    }
}

/// On-disk shape of one retained per-request cache tuple. Mirrors
/// RequestUsage field for field so types.rs stays Serialize-only.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
struct RequestUsageMirror {
    ts: u64,
    input_tokens: u64,
    cache_read_tokens: u64,
    cache_write_5m_tokens: u64,
    cache_write_1h_tokens: u64,
}

impl From<&RequestUsage> for RequestUsageMirror {
    fn from(r: &RequestUsage) -> Self {
        RequestUsageMirror {
            ts: r.ts,
            input_tokens: r.input_tokens,
            cache_read_tokens: r.cache_read_tokens,
            cache_write_5m_tokens: r.cache_write_5m_tokens,
            cache_write_1h_tokens: r.cache_write_1h_tokens,
        }
    }
}

impl From<RequestUsageMirror> for RequestUsage {
    fn from(m: RequestUsageMirror) -> Self {
        RequestUsage {
            ts: m.ts,
            input_tokens: m.input_tokens,
            cache_read_tokens: m.cache_read_tokens,
            cache_write_5m_tokens: m.cache_write_5m_tokens,
            cache_write_1h_tokens: m.cache_write_1h_tokens,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SessionUsageMirror {
    source: String,
    session_id: String,
    project_key: String,
    first_ts: u64,
    last_ts: u64,
    totals: UsageTotalsMirror,
    by_model: BTreeMap<String, UsageTotalsMirror>,
    by_day: BTreeMap<String, UsageTotalsMirror>,
    /// Always empty for Codex today (the only payload kind that caches whole
    /// sessions), but mirrored fully so a future reader that retains Codex
    /// requests round-trips instead of silently dropping them. `default` so
    /// the field is not load-bearing for deserialization.
    #[serde(default)]
    requests: Vec<RequestUsageMirror>,
}

fn source_to_str(s: UsageSource) -> &'static str {
    match s {
        UsageSource::ClaudeCode => "claude_code",
        UsageSource::Codex => "codex",
    }
}

fn source_from_str(s: &str) -> Option<UsageSource> {
    match s {
        "claude_code" => Some(UsageSource::ClaudeCode),
        "codex" => Some(UsageSource::Codex),
        _ => None,
    }
}

impl SessionUsageMirror {
    fn into_session(self) -> Option<SessionUsage> {
        Some(SessionUsage {
            source: source_from_str(&self.source)?,
            session_id: self.session_id,
            project_key: self.project_key,
            first_ts: self.first_ts,
            last_ts: self.last_ts,
            totals: self.totals.into(),
            by_model: self
                .by_model
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            by_day: self
                .by_day
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            requests: self.requests.into_iter().map(RequestUsage::from).collect(),
        })
    }
}

impl From<&SessionUsage> for SessionUsageMirror {
    fn from(s: &SessionUsage) -> Self {
        SessionUsageMirror {
            source: source_to_str(s.source).to_string(),
            session_id: s.session_id.clone(),
            project_key: s.project_key.clone(),
            first_ts: s.first_ts,
            last_ts: s.last_ts,
            totals: s.totals.into(),
            by_model: s
                .by_model
                .iter()
                .map(|(k, v)| (k.clone(), (*v).into()))
                .collect(),
            by_day: s
                .by_day
                .iter()
                .map(|(k, v)| (k.clone(), (*v).into()))
                .collect(),
            requests: s.requests.iter().map(RequestUsageMirror::from).collect(),
        }
    }
}

/// On-disk shape of one Claude stage-one record. Mirrors DedupedRecord so
/// types.rs / claude_code.rs do not need to grow extra derives.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct DedupedRecordMirror {
    message_id: String,
    request_id: String,
    ts: u64,
    day: String,
    cwd: String,
    model: String,
    usage: UsageTotalsMirror,
}

impl From<&DedupedRecord> for DedupedRecordMirror {
    fn from(r: &DedupedRecord) -> Self {
        DedupedRecordMirror {
            message_id: r.message_id.clone(),
            request_id: r.request_id.clone(),
            ts: r.ts,
            day: r.day.clone(),
            cwd: r.cwd.clone(),
            model: r.model.clone(),
            usage: r.usage.into(),
        }
    }
}

impl From<DedupedRecordMirror> for DedupedRecord {
    fn from(m: DedupedRecordMirror) -> Self {
        DedupedRecord {
            message_id: m.message_id,
            request_id: m.request_id,
            ts: m.ts,
            day: m.day,
            cwd: m.cwd,
            model: m.model,
            usage: m.usage.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum ParsedPayloadWire {
    #[serde(rename = "claude_records")]
    ClaudeRecords { records: Vec<DedupedRecordMirror> },
    #[serde(rename = "codex_sessions")]
    CodexSessions { sessions: Vec<SessionUsageMirror> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntryWire {
    mtime_secs: u64,
    size_bytes: u64,
    skipped_lines: u64,
    source: String,
    payload: ParsedPayloadWire,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheFileWire {
    v: u32,
    entries: BTreeMap<String, CacheEntryWire>,
}

impl CacheEntryWire {
    fn into_entry(self) -> Option<CacheEntry> {
        let source = source_from_str(&self.source)?;
        let payload = match self.payload {
            ParsedPayloadWire::ClaudeRecords { records } => {
                if !matches!(source, UsageSource::ClaudeCode) {
                    return None;
                }
                ParsedPayload::Claude {
                    records: records.into_iter().map(DedupedRecord::from).collect(),
                }
            }
            ParsedPayloadWire::CodexSessions { sessions } => {
                if !matches!(source, UsageSource::Codex) {
                    return None;
                }
                let mut out = Vec::with_capacity(sessions.len());
                for s in sessions {
                    out.push(s.into_session()?);
                }
                ParsedPayload::Codex { sessions: out }
            }
        };
        Some(CacheEntry {
            mtime_secs: self.mtime_secs,
            size_bytes: self.size_bytes,
            skipped_lines: self.skipped_lines,
            source,
            payload,
        })
    }
}

impl From<&CacheEntry> for CacheEntryWire {
    fn from(e: &CacheEntry) -> Self {
        let payload = match &e.payload {
            ParsedPayload::Claude { records } => ParsedPayloadWire::ClaudeRecords {
                records: records.iter().map(DedupedRecordMirror::from).collect(),
            },
            ParsedPayload::Codex { sessions } => ParsedPayloadWire::CodexSessions {
                sessions: sessions.iter().map(SessionUsageMirror::from).collect(),
            },
        };
        CacheEntryWire {
            mtime_secs: e.mtime_secs,
            size_bytes: e.size_bytes,
            skipped_lines: e.skipped_lines,
            source: source_to_str(e.source).to_string(),
            payload,
        }
    }
}

/// Read both providers' logs through the cache. Either input can be `None` to
/// skip that provider. Returns one combined UsageData. The cache file lives in
/// `cache_dir/usage-cache.json`; any I/O or version mismatch downgrades to a
/// full reparse without surfacing an error.
///
/// For Claude Code the cache stores per-file stage-one records; the global
/// stage-two merge is rerun on every invocation so a brand-new file that
/// duplicates an old key still collapses correctly. Codex caches one
/// SessionUsage per rollout (no cross-file dedup needed: rollout files do not
/// share message ids).
pub fn read_all_cached(
    claude_dir: Option<&Path>,
    codex_dir: Option<&Path>,
    cache_dir: &Path,
) -> UsageData {
    let mut cache = load_cache(cache_dir);
    let mut next = CacheFile::default();
    let mut data = UsageData::default();
    let mut claude_records: Vec<(PathBuf, Vec<DedupedRecord>)> = Vec::new();

    if let Some(dir) = claude_dir {
        ingest_claude(dir, &mut cache, &mut next, &mut data, &mut claude_records);
    }
    if let Some(dir) = codex_dir {
        ingest_codex(dir, &mut cache, &mut next, &mut data);
    }

    // Stage two: global cross-file dedup across every Claude file. This MUST
    // run fresh per invocation, even on a full cache hit, because cache hits
    // restore records but the winner of a (message_id, request_id) tie can
    // shift when another file's records change.
    claude_code::aggregate_records(claude_records, &mut data);

    let _ = save_cache(cache_dir, &next);
    data
}

fn ingest_claude(
    dir: &Path,
    cache: &mut CacheFile,
    next: &mut CacheFile,
    data: &mut UsageData,
    claude_records: &mut Vec<(PathBuf, Vec<DedupedRecord>)>,
) {
    if !dir.is_dir() {
        return;
    }
    let Ok(projects) = fs::read_dir(dir) else {
        return;
    };
    for proj in projects.flatten() {
        let pp = proj.path();
        if !pp.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&pp) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            handle_claude_file(&path, cache, next, data, claude_records);
        }
    }
}

fn ingest_codex(dir: &Path, cache: &mut CacheFile, next: &mut CacheFile, data: &mut UsageData) {
    if !dir.is_dir() {
        return;
    }
    let Ok(years) = fs::read_dir(dir) else {
        return;
    };
    for y in years.flatten() {
        let yp = y.path();
        if !yp.is_dir() {
            continue;
        }
        let Ok(months) = fs::read_dir(&yp) else {
            continue;
        };
        for m in months.flatten() {
            let mp = m.path();
            if !mp.is_dir() {
                continue;
            }
            let Ok(days) = fs::read_dir(&mp) else {
                continue;
            };
            for d in days.flatten() {
                let dp = d.path();
                if !dp.is_dir() {
                    continue;
                }
                let Ok(files) = fs::read_dir(&dp) else {
                    continue;
                };
                for f in files.flatten() {
                    let path = f.path();
                    let is_rollout = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                        .unwrap_or(false);
                    if !is_rollout {
                        continue;
                    }
                    handle_codex_file(&path, cache, next, data);
                }
            }
        }
    }
}

fn handle_claude_file(
    path: &Path,
    cache: &mut CacheFile,
    next: &mut CacheFile,
    data: &mut UsageData,
    claude_records: &mut Vec<(PathBuf, Vec<DedupedRecord>)>,
) {
    let Some((mtime_secs, size_bytes)) = file_stamp(path) else {
        data.skipped_files += 1;
        return;
    };
    let key = path.to_string_lossy().to_string();
    if let Some(entry) = cache.entries.get(&key) {
        if matches!(entry.source, UsageSource::ClaudeCode)
            && entry.mtime_secs == mtime_secs
            && entry.size_bytes == size_bytes
        {
            if let ParsedPayload::Claude { records } = &entry.payload {
                data.skipped_lines += entry.skipped_lines;
                claude_records.push((path.to_path_buf(), records.clone()));
                next.entries.insert(key, entry.clone());
                return;
            }
        }
    }
    let mut local_skipped_lines = 0u64;
    let mut local_skipped_files = 0u64;
    let records =
        claude_code::parse_file_records(path, &mut local_skipped_lines, &mut local_skipped_files);
    data.skipped_lines += local_skipped_lines;
    data.skipped_files += local_skipped_files;
    claude_records.push((path.to_path_buf(), records.clone()));
    next.entries.insert(
        key,
        CacheEntry {
            mtime_secs,
            size_bytes,
            skipped_lines: local_skipped_lines,
            source: UsageSource::ClaudeCode,
            payload: ParsedPayload::Claude { records },
        },
    );
}

fn handle_codex_file(
    path: &Path,
    cache: &mut CacheFile,
    next: &mut CacheFile,
    data: &mut UsageData,
) {
    let Some((mtime_secs, size_bytes)) = file_stamp(path) else {
        data.skipped_files += 1;
        return;
    };
    let key = path.to_string_lossy().to_string();
    if let Some(entry) = cache.entries.get(&key) {
        if matches!(entry.source, UsageSource::Codex)
            && entry.mtime_secs == mtime_secs
            && entry.size_bytes == size_bytes
        {
            if let ParsedPayload::Codex { sessions } = &entry.payload {
                data.sessions.extend(sessions.iter().cloned());
                data.skipped_lines += entry.skipped_lines;
                next.entries.insert(key, entry.clone());
                return;
            }
        }
    }
    let mut local_skipped_lines = 0u64;
    let mut local_skipped_files = 0u64;
    let sessions: Vec<SessionUsage> =
        codex::parse_file(path, &mut local_skipped_lines, &mut local_skipped_files)
            .into_iter()
            .collect();
    data.sessions.extend(sessions.iter().cloned());
    data.skipped_lines += local_skipped_lines;
    data.skipped_files += local_skipped_files;
    next.entries.insert(
        key,
        CacheEntry {
            mtime_secs,
            size_bytes,
            skipped_lines: local_skipped_lines,
            source: UsageSource::Codex,
            payload: ParsedPayload::Codex { sessions },
        },
    );
}

fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some((mtime, size))
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

fn load_cache(dir: &Path) -> CacheFile {
    let path = cache_path(dir);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return CacheFile::default(),
    };
    let wire: CacheFileWire = match serde_json::from_str(&text) {
        Ok(w) => w,
        Err(_) => return CacheFile::default(),
    };
    if wire.v != CACHE_VERSION {
        return CacheFile::default();
    }
    let mut entries = BTreeMap::new();
    for (k, e) in wire.entries {
        if let Some(entry) = e.into_entry() {
            entries.insert(k, entry);
        }
        // Drop entries we cannot reconstruct (unknown source string, or
        // mismatched payload kind); they get re-parsed on this run.
    }
    CacheFile { v: wire.v, entries }
}

fn save_cache(dir: &Path, cf: &CacheFile) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let wire = CacheFileWire {
        v: cf.v,
        entries: cf
            .entries
            .iter()
            .map(|(k, e)| (k.clone(), CacheEntryWire::from(e)))
            .collect(),
    };
    let text = serde_json::to_string(&wire).map_err(std::io::Error::other)?;
    fs::write(cache_path(dir), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-usage-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixtures_claude() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("usage")
            .join("claude-code")
    }

    fn fixtures_codex() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("usage")
            .join("codex")
    }

    #[test]
    fn cold_cache_parses_and_warms() {
        let cache = tmp("warm");
        let data = read_all_cached(Some(&fixtures_claude()), Some(&fixtures_codex()), &cache);
        assert!(!data.sessions.is_empty());
        assert!(cache.join(CACHE_FILE).exists());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn warm_cache_round_trip_matches_cold() {
        let cache = tmp("matches");
        let cold = read_all_cached(Some(&fixtures_claude()), Some(&fixtures_codex()), &cache);
        let warm = read_all_cached(Some(&fixtures_claude()), Some(&fixtures_codex()), &cache);
        assert_eq!(cold.sessions.len(), warm.sessions.len());
        let cold_total: u64 = cold.sessions.iter().map(|s| s.totals.output_tokens).sum();
        let warm_total: u64 = warm.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(cold_total, warm_total);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn warm_cache_matches_fresh_claude_code_read() {
        // The cached path and the direct read() path MUST agree, including
        // the cross-file dedup result, since they share stage two now.
        let cache = tmp("agree");
        let cached = read_all_cached(Some(&fixtures_claude()), None, &cache);
        let direct = claude_code::read(&fixtures_claude());
        assert_eq!(cached.sessions.len(), direct.sessions.len());
        let cached_total: u64 = cached.sessions.iter().map(|s| s.totals.output_tokens).sum();
        let direct_total: u64 = direct.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(cached_total, direct_total);
        let cached_in: u64 = cached.sessions.iter().map(|s| s.totals.input_tokens).sum();
        let direct_in: u64 = direct.sessions.iter().map(|s| s.totals.input_tokens).sum();
        assert_eq!(cached_in, direct_in);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn size_change_invalidates_entry() {
        // The cache keys on (mtime_secs, size_bytes); we exercise the size
        // half because filesystem mtime resolution is too coarse in CI for a
        // single-second guarantee, and bumping mtime portably is not in std.
        let work = tmp("size");
        let project = work.join("proj");
        fs::create_dir_all(&project).unwrap();
        let log = project.join("ses1.jsonl");
        // Two synthetic single-record payloads of different lengths.
        let line_100 = concat!(
            r#"{"message":{"id":"m1","model":"claude-sonnet-4-6","usage":"#,
            r#"{"input_tokens":10,"output_tokens":100,"cache_read_input_tokens":0,"#,
            r#""cache_creation_input_tokens":0}},"requestId":"r1","#,
            r#""cwd":"/x","timestamp":"2026-06-10T00:00:00Z"}"#,
        );
        // Output 200000 keeps payload size different from line_100.
        let line_200 = concat!(
            r#"{"message":{"id":"m1","model":"claude-sonnet-4-6","usage":"#,
            r#"{"input_tokens":10,"output_tokens":200000,"cache_read_input_tokens":0,"#,
            r#""cache_creation_input_tokens":0}},"requestId":"r1","#,
            r#""cwd":"/x","timestamp":"2026-06-10T00:00:00Z"}"#,
        );
        fs::write(&log, line_100).unwrap();
        let cache = tmp("size-cache");
        let d1 = read_all_cached(Some(&work), None, &cache);
        let sum1: u64 = d1.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(sum1, 100);

        fs::write(&log, line_200).unwrap();
        let d2 = read_all_cached(Some(&work), None, &cache);
        let sum2: u64 = d2.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(sum2, 200_000, "size change should re-parse");

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn cross_file_dedup_survives_cache_round_trip() {
        // Two files in two projects share (m, r); cold and warm reads must
        // both collapse the duplicate, even though the cache is keyed per
        // file. Stage two ALWAYS runs fresh, so global merge is always in
        // play.
        let work = tmp("xfile-cache");
        let p1 = work.join("p1");
        let p2 = work.join("p2");
        fs::create_dir_all(&p1).unwrap();
        fs::create_dir_all(&p2).unwrap();
        let dup = concat!(
            r#"{"message":{"id":"shared","model":"claude-sonnet-4-6","usage":"#,
            r#"{"input_tokens":10,"output_tokens":42,"cache_read_input_tokens":0,"#,
            r#""cache_creation_input_tokens":0}},"requestId":"req","#,
            r#""cwd":"/work/shared","timestamp":"2026-06-10T00:00:00Z"}"#,
        );
        fs::write(p1.join("a.jsonl"), dup).unwrap();
        fs::write(p2.join("b.jsonl"), dup).unwrap();
        let cache = tmp("xfile-cache-store");
        let cold = read_all_cached(Some(&work), None, &cache);
        let cold_total: u64 = cold.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(
            cold_total, 42,
            "global merge collapses duplicate on cold read"
        );
        let warm = read_all_cached(Some(&work), None, &cache);
        let warm_total: u64 = warm.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(
            warm_total, 42,
            "global merge collapses duplicate on warm read"
        );
        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn unchanged_file_serves_from_cache() {
        // A second pass over identical files must reuse cached parses. We
        // verify by mutating the on-disk cache for one Claude file to inject
        // a sentinel record, then confirming the warm read surfaces it. The
        // sentinel uses a unique (message_id, request_id) so it survives
        // global dedup.
        let cache = tmp("hit");
        let cold = read_all_cached(Some(&fixtures_claude()), None, &cache);
        assert!(!cold.sessions.is_empty());
        let mut wire: CacheFileWire =
            serde_json::from_str(&fs::read_to_string(cache.join(CACHE_FILE)).unwrap()).unwrap();
        let first_key = wire.entries.keys().next().cloned().unwrap();
        let entry = wire.entries.get_mut(&first_key).unwrap();
        let sentinel = DedupedRecordMirror {
            message_id: "SENTINEL_MSG".to_string(),
            request_id: "SENTINEL_REQ".to_string(),
            ts: 1_781_108_711,
            day: "2026-06-10".to_string(),
            cwd: "/sentinel".to_string(),
            model: "claude-sentinel".to_string(),
            usage: UsageTotalsMirror {
                output_tokens: 4_242_424,
                ..UsageTotalsMirror::default()
            },
        };
        entry.payload = ParsedPayloadWire::ClaudeRecords {
            records: vec![sentinel],
        };
        fs::write(
            cache.join(CACHE_FILE),
            serde_json::to_string(&wire).unwrap(),
        )
        .unwrap();
        let warm = read_all_cached(Some(&fixtures_claude()), None, &cache);
        let has_sentinel = warm
            .sessions
            .iter()
            .any(|s| s.project_key == "/sentinel" && s.totals.output_tokens == 4_242_424);
        assert!(has_sentinel, "warm read should serve the cached sentinel");
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn corrupt_cache_file_falls_back_to_fresh_parse() {
        let cache = tmp("corrupt");
        fs::write(cache.join(CACHE_FILE), "{not json").unwrap();
        let data = read_all_cached(Some(&fixtures_claude()), None, &cache);
        assert!(!data.sessions.is_empty());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn version_mismatch_treated_as_cold() {
        let cache = tmp("vmismatch");
        fs::write(cache.join(CACHE_FILE), r#"{"v":999,"entries":{}}"#).unwrap();
        let data = read_all_cached(Some(&fixtures_claude()), None, &cache);
        assert!(!data.sessions.is_empty());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn v1_cache_is_ignored_and_full_reparse_happens() {
        // A v1 cache file (the pre-fix shape) must not be loaded; the cache
        // must silently treat the run as cold. We write a minimally valid v1
        // file (matching the OLD shape: entries with sessions: [], etc) and
        // confirm the run still produces correct results and replaces the
        // file with a current-version cache on write-back.
        let cache = tmp("v1-ignored");
        // The exact v1 shape is not load-bearing here; the only contract is
        // "version != CACHE_VERSION means cold". We write that minimally.
        let v1_blob = r#"{"v":1,"entries":{"/some/path":{"mtime_secs":0,"size_bytes":0,"sessions":[],"skipped_lines":0,"source":"claude_code"}}}"#;
        fs::write(cache.join(CACHE_FILE), v1_blob).unwrap();
        let data = read_all_cached(Some(&fixtures_claude()), None, &cache);
        assert!(!data.sessions.is_empty(), "v1 cache must trigger reparse");
        // After save_cache, the file is on the current version.
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(cache.join(CACHE_FILE)).unwrap()).unwrap();
        assert_eq!(
            written.get("v").and_then(|v| v.as_u64()),
            Some(CACHE_VERSION as u64)
        );
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn v2_cache_self_heals_by_silent_reparse() {
        // A v2 cache (the pre-retention shape: Claude stage-one records, no
        // requests vectors anywhere) must be treated as cold so the run
        // rebuilds per-request retention from the logs, then write back v3.
        // We write a structurally valid v2 file whose payload would parse
        // under today's wire types if the version gate were skipped; the
        // sentinel inside must NOT surface, proving the gate (not a parse
        // failure) rejected it.
        let cache = tmp("v2-self-heal");
        // Key the stale entry on a REAL fixture file with its REAL stamp, so
        // that if the version gate were skipped the sentinel would be served
        // as a cache hit. Only the version gate stands between the sentinel
        // and the output.
        let fixture_file = fixtures_claude()
            .join("proj-a")
            .join("streaming-and-cwd.jsonl");
        let (mtime_secs, size_bytes) = file_stamp(&fixture_file).expect("fixture stamp");
        let v2_blob = format!(
            concat!(
                r#"{{"v":2,"entries":{{"{key}":{{"mtime_secs":{mtime},"size_bytes":{size},"#,
                r#""skipped_lines":0,"source":"claude_code","payload":{{"kind":"claude_records","#,
                r#""records":[{{"message_id":"V2_SENTINEL","request_id":"V2_SENTINEL","ts":1781108711,"#,
                r#""day":"2026-06-10","cwd":"/v2-sentinel","model":"claude-sentinel","#,
                r#""usage":{{"input_tokens":0,"output_tokens":777,"cache_read_tokens":0,"#,
                r#""cache_write_5m_tokens":0,"cache_write_1h_tokens":0}}}}]}}}}}}}}"#,
            ),
            key = fixture_file.to_string_lossy().replace('\\', "\\\\"),
            mtime = mtime_secs,
            size = size_bytes,
        );
        fs::write(cache.join(CACHE_FILE), v2_blob).unwrap();
        let data = read_all_cached(Some(&fixtures_claude()), None, &cache);
        assert!(!data.sessions.is_empty(), "v2 cache must trigger reparse");
        assert!(
            !data
                .sessions
                .iter()
                .any(|s| s.project_key == "/v2-sentinel"),
            "v2 entries must not be served"
        );
        // Reparsed sessions carry retention (the fixture has usage records).
        assert!(
            data.sessions
                .iter()
                .filter(|s| matches!(s.source, UsageSource::ClaudeCode))
                .all(|s| !s.requests.is_empty()),
            "reparse must rebuild per-request retention"
        );
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(cache.join(CACHE_FILE)).unwrap()).unwrap();
        assert_eq!(written.get("v").and_then(|v| v.as_u64()), Some(3));
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn warm_cache_preserves_per_request_retention() {
        // Retention is derived in stage two from cached stage-one records, so
        // a warm read must produce exactly the same request tuples as a cold
        // read, in the same order.
        let cache = tmp("retention-roundtrip");
        let cold = read_all_cached(Some(&fixtures_claude()), None, &cache);
        let warm = read_all_cached(Some(&fixtures_claude()), None, &cache);
        let pick = |data: &UsageData| -> Vec<(String, String, Vec<RequestUsage>)> {
            let mut rows: Vec<_> = data
                .sessions
                .iter()
                .map(|s| {
                    (
                        s.session_id.clone(),
                        s.project_key.clone(),
                        s.requests.clone(),
                    )
                })
                .collect();
            rows.sort();
            rows
        };
        assert_eq!(pick(&cold), pick(&warm));
        assert!(
            cold.sessions.iter().any(|s| !s.requests.is_empty()),
            "fixture sessions should retain requests"
        );
        let _ = fs::remove_dir_all(&cache);
    }
}
