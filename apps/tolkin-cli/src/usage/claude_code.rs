//! Claude Code session log reader.
//!
//! # Layout
//!
//! Parent sessions:
//!   `<projects_dir>/<slug>/<session_id>.jsonl`
//!
//! Subagent streams (Task-tool fan-out, one file per spawned subagent):
//!   `<projects_dir>/<slug>/<session_id>/subagents/agent-<agent_id>.jsonl`
//!
//! Both shapes carry assistant records with the same field set: `cwd`,
//! `requestId`, `timestamp`, `message.id`, `message.model`, `message.usage`.
//! We attribute by the per-record `cwd` (NOT by reversing the directory
//! slug) because users `cd` mid-session, and subagent records observed in
//! the wild ALWAYS carry their own `cwd` (a 47-session, 299-file, 13.8K
//! record probe found zero records missing the field). If a subagent ever
//! ships without `cwd` we fall through the same "extract returns None"
//! gate used by parent records, so no special-cased fallback is required.
//!
//! # Why subagents are their own SessionUsage rows
//!
//! Each subagent file is its own prompt stream: its own system prompt, its
//! own growing prefix, its own cache lifetime. Folding subagent requests
//! into the parent session's request vector would interleave unrelated
//! prefixes and corrupt churn / TTL / gap math. So we emit one SessionUsage
//! per subagent file. The discrimination lives in
//! [`UsageData::subagent_session_keys`] (not on SessionUsage itself, so
//! downstream code that already constructs SessionUsage via struct
//! literals stays unchanged). The headline "working session" count uses
//! `UsageData::parent_session_count()`; the fan-out spend is reachable via
//! `subagent_stream_count()`.
//!
//! # Disjointness with parent records
//!
//! Subagent transcripts are written from the subagent's own request/response
//! stream, not duplicated into the parent session jsonl. An exhaustive
//! probe of one workstation's logs (47 parent sessions with subagents,
//! 299 subagent files) found zero (`message.id`, `requestId`) overlap
//! between any parent and any subagent. The global last-wins dedup spans
//! BOTH file kinds anyway, so a future Claude Code release that DID
//! duplicate records would collapse safely under the existing winner
//! rule; the unit test `subagent_records_disjoint_from_parent_in_practice`
//! pins the observed shape and a synthetic-collision test exercises the
//! dedup-collapses-collision path.
//!
//! # Two-stage ingestion
//!
//! Stage one parses one file and returns its within-file last-wins deduped
//! records. Stage two merges every file's records (parent and subagent)
//! into one global dedup map keyed on (`message.id`, `requestId`), then
//! groups winners by (is_subagent, file stem, cwd) into [`SessionUsage`].
//!
//! Within-file dedup contract: streaming writes emit 3 to 4 intermediate
//! snapshots with the same (`message.id`, `requestId`) pair where
//! `output_tokens` grows or stays equal. We keep the LAST occurrence per file.
//! First-wins undercounts (the known ccusage bug #866/#888) and we do not
//! import that behavior.
//!
//! Cross-file dedup contract: Claude Code session resume and branching copies
//! earlier transcript records verbatim into a NEW jsonl file with the same
//! (`message.id`, `requestId`). Per-file scoping would count the same API
//! response twice. Global merge picks one winner per key: larger `ts` wins;
//! on a tie, larger `output_tokens` wins; on a further tie, the record from
//! the lexicographically first file path wins (deterministic across runs).
//! The winner keeps its own session_id and cwd attribution, so a fully-copied
//! older file may end up emitting zero SessionUsage rows. That is acceptable
//! and arguably correct: the resume is one logical session.
//!
//! Bad timestamp policy: a record with a missing or unparseable timestamp is
//! counted as a skipped LINE and excluded entirely. Keeping it without a date
//! would corrupt the `by_day` map; assigning it to epoch 0 would corrupt
//! `first_ts`. Synthetic-model records are filtered before dedup so they never
//! displace a real record by id collision.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::types::{RequestUsage, SessionUsage, UsageData, UsageSource, UsageTotals};

/// One within-file deduped record. Stage one returns these per file; stage two
/// merges them globally. Cache stores these (token counts, timestamps, opaque
/// ids, cwd strings only; no message content). `UsageTotals` is Serialize-only
/// by the frozen contract, so this struct holds the totals as flat u64 fields
/// the cache layer can round-trip through serde.
#[derive(Clone, Debug)]
pub struct DedupedRecord {
    /// `message.id` from the log.
    pub message_id: String,
    /// `requestId` from the log.
    pub request_id: String,
    /// Unix epoch seconds.
    pub ts: u64,
    /// UTC "YYYY-MM-DD" derived from `ts`.
    pub day: String,
    /// Per-record cwd as recorded.
    pub cwd: String,
    /// Raw model id (not normalized for pricing).
    pub model: String,
    /// Token totals for this record.
    pub usage: UsageTotals,
}

/// Walks parent session files and subagent transcripts:
///
///   `<projects_dir>/<slug>/*.jsonl`                       (parent sessions)
///   `<projects_dir>/<slug>/<session>/subagents/agent-*.jsonl`  (subagents)
///
/// Missing or unreadable dir produces an empty UsageData. One file may
/// contribute multiple SessionUsage records (one per distinct surviving cwd
/// inside it). Runs stage one per file and stage two over the combined
/// record set; the cache layer can reuse stage one and feed stage two from
/// cached records.
#[allow(dead_code)] // direct no-cache entry, exercised by tests; production path is cache::read_all_cached
pub fn read(projects_dir: &Path) -> UsageData {
    let mut data = UsageData::default();
    if !projects_dir.is_dir() {
        return data;
    }
    let Ok(entries) = fs::read_dir(projects_dir) else {
        return data;
    };
    let mut per_file: Vec<(PathBuf, Vec<DedupedRecord>)> = Vec::new();
    for entry in entries.flatten() {
        let project_dir = entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&project_dir) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.is_dir() {
                // A `<session_id>/` directory beside its `<session_id>.jsonl`
                // file holds subagent transcripts under `subagents/`. Other
                // sibling subdirectories (none observed today; left harmless)
                // are ignored.
                let subagents_dir = path.join("subagents");
                if subagents_dir.is_dir() {
                    let Ok(subs) = fs::read_dir(&subagents_dir) else {
                        continue;
                    };
                    for sub in subs.flatten() {
                        let sub_path = sub.path();
                        if !is_subagent_jsonl(&sub_path) {
                            // Subagent dirs sometimes carry `*.meta.json`
                            // companions; skip everything but the jsonl
                            // streams the parser is built for.
                            continue;
                        }
                        let records = parse_file_records(
                            &sub_path,
                            &mut data.skipped_lines,
                            &mut data.skipped_files,
                        );
                        per_file.push((sub_path, records));
                    }
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let records =
                parse_file_records(&path, &mut data.skipped_lines, &mut data.skipped_files);
            per_file.push((path, records));
        }
    }
    aggregate_records(per_file, &mut data);
    data
}

/// Returns true for `agent-<id>.jsonl` files inside a `subagents/` dir.
/// Excludes the `.meta.json` companions Claude Code drops next to each
/// subagent transcript.
pub fn is_subagent_jsonl(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with("agent-") && name.ends_with(".jsonl")
}

/// Inspect a file path and return the parent_session_id if the path
/// matches the subagent layout (`.../<parent_session_id>/subagents/agent-*.jsonl`),
/// or None for a parent session file. Used by stage two to tag each
/// SessionUsage with its provenance without re-walking directories.
pub fn parent_session_id_from_path(path: &Path) -> Option<String> {
    // We need to detect that the immediate grandparent dir is "subagents".
    let parent = path.parent()?;
    if parent.file_name().and_then(|n| n.to_str()) != Some("subagents") {
        return None;
    }
    let grandparent = parent.parent()?;
    grandparent
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Stage one: parse one Claude Code session JSONL into its within-file
/// last-wins deduped records. Synthetic-model records and records with bad
/// timestamps are dropped pre-dedup. Public so the cache layer can reuse the
/// parser and store its result.
pub fn parse_file_records(
    path: &Path,
    skipped_lines: &mut u64,
    skipped_files: &mut u64,
) -> Vec<DedupedRecord> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            *skipped_files += 1;
            return Vec::new();
        }
    };

    let mut dedup: BTreeMap<(String, String), DedupedRecord> = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                *skipped_lines += 1;
                continue;
            }
        };
        let Some(raw) = extract_record(&v) else {
            // Not a usage-bearing assistant record on a non-synthetic model.
            continue;
        };
        let Some(ts) = parse_iso8601(&raw.timestamp) else {
            // Skip records we cannot place on a calendar day.
            *skipped_lines += 1;
            continue;
        };
        let day = utc_day_from_secs(ts);
        let record = DedupedRecord {
            message_id: raw.message_id,
            request_id: raw.request_id,
            ts,
            day,
            cwd: raw.cwd,
            model: raw.model,
            usage: raw.usage,
        };
        let key = (record.message_id.clone(), record.request_id.clone());
        if dedup.insert(key.clone(), record).is_none() {
            order.push(key);
        }
    }

    // Preserve first-seen order so the cache file is deterministic across
    // runs even when the underlying BTreeMap iteration would not be.
    let mut out = Vec::with_capacity(order.len());
    for key in order {
        if let Some(rec) = dedup.remove(&key) {
            out.push(rec);
        }
    }
    out
}

/// Stage two: merge per-file deduped records globally on (message_id,
/// request_id), then group winners by (is_subagent, session_id, cwd) into
/// SessionUsage.
///
/// Winner rule across files for the same key:
/// 1. larger `ts` wins;
/// 2. tie on `ts`: larger `output_tokens` wins;
/// 3. further tie: lexicographically first file path wins.
///
/// Each file contributes its `session_id` as follows:
/// - parent session file stem (e.g. `<uuid>.jsonl` -> `<uuid>`), or
/// - subagent file stem (e.g. `agent-<id>.jsonl` -> `agent-<id>`).
///
/// Subagent-derived SessionUsage rows are recorded in
/// `UsageData::subagent_session_keys` and the originating parent's id is
/// recorded in `UsageData::subagent_parent_ids`, so a surface can present
/// fan-out spend without conflating it with the parent's working-session
/// count. The SessionUsage struct itself stays unchanged (the
/// discrimination lives next to the collection it discriminates, not on
/// every member). Inputs are taken by value because the cache layer fills
/// them by cloning out of cache entries already.
pub fn aggregate_records(per_file: Vec<(PathBuf, Vec<DedupedRecord>)>, data: &mut UsageData) {
    // Stable file-path sort so the tie-break is deterministic. The outer Vec
    // is the only source of "which file did this come from" information, so
    // we keep the path bound to each record while merging.
    let mut files = per_file;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    // For each key, remember the winning record plus enough provenance to
    // resolve a future tie deterministically.
    struct Winner {
        record: DedupedRecord,
        file_index: usize,
        session_id: String,
        is_subagent: bool,
        parent_session_id: Option<String>,
    }
    let mut winners: BTreeMap<(String, String), Winner> = BTreeMap::new();
    for (idx, (path, records)) in files.iter().enumerate() {
        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let parent_session_id = parent_session_id_from_path(path);
        let is_subagent = parent_session_id.is_some();
        for rec in records {
            let key = (rec.message_id.clone(), rec.request_id.clone());
            match winners.get(&key) {
                None => {
                    winners.insert(
                        key,
                        Winner {
                            record: rec.clone(),
                            file_index: idx,
                            session_id: session_id.clone(),
                            is_subagent,
                            parent_session_id: parent_session_id.clone(),
                        },
                    );
                }
                Some(existing) => {
                    if record_beats(rec, idx, &existing.record, existing.file_index) {
                        winners.insert(
                            key,
                            Winner {
                                record: rec.clone(),
                                file_index: idx,
                                session_id: session_id.clone(),
                                is_subagent,
                                parent_session_id: parent_session_id.clone(),
                            },
                        );
                    }
                }
            }
        }
    }

    // Group winners by (is_subagent, session_id, cwd) and emit SessionUsage
    // rows in a deterministic order driven by the key. The is_subagent flag
    // bucket-discriminates parent vs subagent files that might share a stem
    // only if a parent session was literally named `agent-<hex>` (vanishingly
    // unlikely; the discriminator stays cheap insurance). We carry the
    // parent_session_id alongside each grouped entry so the side-tables on
    // UsageData can be populated after this loop without a second pass.
    let mut grouped: BTreeMap<(bool, String, String), (SessionUsage, Option<String>)> =
        BTreeMap::new();
    for winner in winners.into_values() {
        let totals = winner.record.usage;
        let key = (
            winner.is_subagent,
            winner.session_id.clone(),
            winner.record.cwd.clone(),
        );
        let parent_id = winner.parent_session_id.clone();
        let entry = grouped.entry(key).or_insert_with(|| {
            (
                SessionUsage {
                    source: UsageSource::ClaudeCode,
                    session_id: winner.session_id.clone(),
                    project_key: winner.record.cwd.clone(),
                    first_ts: winner.record.ts,
                    last_ts: winner.record.ts,
                    totals: UsageTotals::default(),
                    by_model: BTreeMap::new(),
                    by_day: BTreeMap::new(),
                    requests: Vec::new(),
                },
                parent_id.clone(),
            )
        });
        let session = &mut entry.0;
        session.totals.add(&totals);
        session
            .by_model
            .entry(winner.record.model.clone())
            .or_default()
            .add(&totals);
        session
            .by_day
            .entry(winner.record.day.clone())
            .or_default()
            .add(&totals);
        // Per-request retention for cache analysis. This happens AFTER the
        // global cross-file dedup above, so a resumed session's copied
        // records contribute exactly one tuple each. Subagent rows retain
        // their own stream's tuples too: each subagent has its own prefix
        // and the cache analyzer treats each as its own timeline.
        session.requests.push(RequestUsage {
            ts: winner.record.ts,
            input_tokens: totals.input_tokens,
            cache_read_tokens: totals.cache_read_tokens,
            cache_write_5m_tokens: totals.cache_write_5m_tokens,
            cache_write_1h_tokens: totals.cache_write_1h_tokens,
        });
        if winner.record.ts < session.first_ts {
            session.first_ts = winner.record.ts;
        }
        if winner.record.ts > session.last_ts {
            session.last_ts = winner.record.ts;
        }
    }
    for (session, _) in grouped.values_mut() {
        // Deterministic within-session request order: ascending by the full
        // tuple (ts first). Log files can carry out-of-order timestamps and
        // the winners map iterates by (message_id, request_id), neither of
        // which is time order.
        session.requests.sort_unstable();
    }
    // Populate side-tables BEFORE moving sessions out of the grouped map.
    for ((is_subagent, _, _), (session, parent_id)) in &grouped {
        if *is_subagent {
            let key = (session.session_id.clone(), session.project_key.clone());
            data.subagent_session_keys.insert(key.clone());
            if let Some(pid) = parent_id {
                data.subagent_parent_ids.insert(key, pid.clone());
            }
        }
    }
    data.sessions
        .extend(grouped.into_values().map(|(session, _)| session));
}

/// Returns true if `cand` from file index `cand_idx` should replace `held`
/// from file index `held_idx` under the cross-file winner rule.
fn record_beats(
    cand: &DedupedRecord,
    cand_idx: usize,
    held: &DedupedRecord,
    held_idx: usize,
) -> bool {
    if cand.ts != held.ts {
        return cand.ts > held.ts;
    }
    if cand.usage.output_tokens != held.usage.output_tokens {
        return cand.usage.output_tokens > held.usage.output_tokens;
    }
    // Lexicographically first file path wins. `files` was sorted ascending,
    // so the smaller index is the "first" path.
    cand_idx < held_idx
}

/// Pre-dedup record straight from JSON; no calendar resolution yet.
struct RawRecord {
    message_id: String,
    request_id: String,
    timestamp: String,
    cwd: String,
    model: String,
    usage: UsageTotals,
}

/// Return Some only for usage-bearing assistant records on a non-synthetic
/// model. Filters happen before timestamp parsing so a malformed-but-irrelevant
/// line does not pollute the skipped-lines counter.
fn extract_record(v: &Value) -> Option<RawRecord> {
    let message = v.get("message")?;
    let usage_v = message.get("usage")?;
    let model = message.get("model").and_then(|m| m.as_str())?;
    if model == "<synthetic>" {
        return None;
    }
    let message_id = message.get("id").and_then(|m| m.as_str())?.to_string();
    let request_id = v.get("requestId").and_then(|r| r.as_str())?.to_string();
    let timestamp = v.get("timestamp").and_then(|t| t.as_str())?.to_string();
    let cwd = v.get("cwd").and_then(|c| c.as_str())?.to_string();

    let input_tokens = usage_v
        .get("input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let output_tokens = usage_v
        .get("output_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cache_read_tokens = usage_v
        .get("cache_read_input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cache_creation_total = usage_v
        .get("cache_creation_input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);

    // Prefer the explicit 5m/1h split when present; otherwise dump everything
    // into the 5m bucket (matches Anthropic's pre-1h-cache rate).
    let (cache_write_5m_tokens, cache_write_1h_tokens) = match usage_v.get("cache_creation") {
        Some(c) => {
            let m5 = c
                .get("ephemeral_5m_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let h1 = c
                .get("ephemeral_1h_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            (m5, h1)
        }
        None => (cache_creation_total, 0),
    };

    Some(RawRecord {
        message_id,
        request_id,
        timestamp,
        cwd,
        model: model.to_string(),
        usage: UsageTotals {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_5m_tokens,
            cache_write_1h_tokens,
        },
    })
}

/// Inverse of Hinnant's days_from_civil. Returns YYYY-MM-DD in UTC for a
/// non-negative unix epoch second count. See
/// howardhinnant.github.io/date_algorithms.html.
pub fn utc_day_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    // Shift so the era boundary is well-defined for non-negative inputs.
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Parse "YYYY-MM-DDTHH:MM:SS(.fff)Z" into epoch seconds. Returns None for any
/// other shape, including non-Z (we do not handle offsets; logs always emit Z).
pub fn parse_iso8601(s: &str) -> Option<u64> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    // Positions: 0123-56-89T12:45:78(...)
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if *bytes.last()? != b'Z' {
        return None;
    }
    let year: i64 = std::str::from_utf8(&bytes[0..4]).ok()?.parse().ok()?;
    let month: u32 = std::str::from_utf8(&bytes[5..7]).ok()?.parse().ok()?;
    let day: u32 = std::str::from_utf8(&bytes[8..10]).ok()?.parse().ok()?;
    let hour: u64 = std::str::from_utf8(&bytes[11..13]).ok()?.parse().ok()?;
    let minute: u64 = std::str::from_utf8(&bytes[14..16]).ok()?.parse().ok()?;
    let second: u64 = std::str::from_utf8(&bytes[17..19]).ok()?.parse().ok()?;
    // Fraction and Z handling: positions 19.. can be ".fff" then 'Z', or just 'Z'.
    // We do not need the fraction value; just sanity-check the shape.
    match bytes[19] {
        b'Z' => {
            if bytes.len() != 20 {
                return None;
            }
        }
        b'.' => {
            // Everything between the dot and the trailing Z must be digits.
            let frac = &bytes[20..bytes.len() - 1];
            if frac.is_empty() || !frac.iter().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        _ => return None,
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        // 60 tolerates a leap second second-of-minute.
        return None;
    }
    let days = days_from_civil(year as i32, month, day);
    let secs = days * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;
    if secs < 0 {
        return None;
    }
    Some(secs as u64)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { (y - 1) as i64 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("usage")
            .join("claude-code")
    }

    #[test]
    fn iso_parser_golden_with_millis() {
        // 2026-06-10T16:25:11.098Z. Hand-verified via Hinnant's formula:
        // days_from_civil(2026,6,10) * 86400 + 16*3600 + 25*60 + 11 = 1781108711.
        let ts = parse_iso8601("2026-06-10T16:25:11.098Z").unwrap();
        assert_eq!(utc_day_from_secs(ts), "2026-06-10");
        assert_eq!(ts, 1_781_108_711);
    }

    #[test]
    fn iso_parser_golden_no_millis() {
        let ts = parse_iso8601("2026-06-10T16:25:11Z").unwrap();
        assert_eq!(ts, 1_781_108_711);
    }

    #[test]
    fn iso_parser_rejects_garbage() {
        assert!(parse_iso8601("").is_none());
        assert!(parse_iso8601("nope").is_none());
        assert!(parse_iso8601("2026-06-10 16:25:11Z").is_none());
        assert!(parse_iso8601("2026-06-10T16:25:11+00:00").is_none());
        assert!(parse_iso8601("2026-13-01T00:00:00Z").is_none());
    }

    #[test]
    fn utc_day_round_trip_epoch() {
        assert_eq!(utc_day_from_secs(0), "1970-01-01");
        assert_eq!(utc_day_from_secs(86_400), "1970-01-02");
    }

    #[test]
    fn empty_dir_produces_empty_usage_data() {
        let dir = std::env::temp_dir().join(format!("tolkin-usage-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = read(&dir);
        assert!(data.sessions.is_empty());
        assert_eq!(data.skipped_lines, 0);
        assert_eq!(data.skipped_files, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_is_empty_not_error() {
        let dir = std::env::temp_dir().join("tolkin-usage-does-not-exist-xyz");
        let _ = std::fs::remove_dir_all(&dir);
        let data = read(&dir);
        assert!(data.sessions.is_empty());
    }

    #[test]
    fn dedup_streaming_keeps_last() {
        let data = read(&fixtures_dir());
        // The streaming-snapshot session lives in proj-a; we expect the LAST
        // snapshot's output_tokens (300), not the first (50) or middle (180).
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "streaming-and-cwd")
            .and_then(|s| {
                // proj-a side
                if s.project_key == "/work/proj-a" {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("proj-a session present");
        // Three streaming snapshots collapse to one record:
        //   output 300, input 10, cache_read 100, cache_write split 40/60.
        // Plus one further record on proj-a with output 90, input 5,
        // cache_read 50, no cache_creation tokens.
        assert_eq!(s.totals.output_tokens, 390);
        assert_eq!(s.totals.input_tokens, 15);
        assert_eq!(s.totals.cache_read_tokens, 150);
        assert_eq!(s.totals.cache_write_5m_tokens, 40);
        assert_eq!(s.totals.cache_write_1h_tokens, 60);
    }

    #[test]
    fn synthetic_model_records_excluded() {
        let data = read(&fixtures_dir());
        // The fixture has a synthetic record with 999 output tokens. If it
        // sneaks in, totals across all sessions would include it.
        let total_output: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert!(total_output < 999, "synthetic record leaked into totals");
    }

    #[test]
    fn one_file_two_cwds_splits_into_two_sessions() {
        let data = read(&fixtures_dir());
        let proj_a = data
            .sessions
            .iter()
            .find(|s| s.session_id == "streaming-and-cwd" && s.project_key == "/work/proj-a");
        let proj_b = data
            .sessions
            .iter()
            .find(|s| s.session_id == "streaming-and-cwd" && s.project_key == "/work/proj-b");
        assert!(proj_a.is_some(), "proj-a session missing");
        assert!(proj_b.is_some(), "proj-b session missing");
        // Both share the same session_id (file stem) but differ on project_key.
        let b = proj_b.unwrap();
        assert_eq!(b.totals.output_tokens, 25);
    }

    #[test]
    fn cache_creation_split_5m_1h_honored() {
        let data = read(&fixtures_dir());
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "streaming-and-cwd" && s.project_key == "/work/proj-a")
            .unwrap();
        // The streaming record carries the explicit split (40 + 60). The
        // proj-b record carries cache_creation_input_tokens 30 with no nested
        // split, which the parser dumps into the 5m bucket; that lands on
        // proj-b and not here.
        assert_eq!(s.totals.cache_write_5m_tokens, 40);
        assert_eq!(s.totals.cache_write_1h_tokens, 60);
    }

    #[test]
    fn bad_lines_counted_skipped_not_fatal() {
        let data = read(&fixtures_dir());
        // The fixture has one bad JSON line plus one record with missing ts.
        assert!(data.skipped_lines >= 2, "expected at least 2 skipped lines");
    }

    #[test]
    fn session_first_and_last_ts_set() {
        let data = read(&fixtures_dir());
        for s in &data.sessions {
            assert!(s.first_ts > 0);
            assert!(s.last_ts >= s.first_ts);
        }
    }

    // Cross-file dedup tests. Each builds a tiny on-disk Claude Code tree
    // under tmp, runs read(), and asserts on the global outcome.

    fn write_record(
        path: &Path,
        message_id: &str,
        request_id: &str,
        ts: &str,
        output: u64,
        cwd: &str,
    ) {
        let line = format!(
            r#"{{"message":{{"model":"claude-sonnet-4-6","id":"{message_id}","usage":{{"input_tokens":1,"output_tokens":{output},"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}},"requestId":"{request_id}","cwd":"{cwd}","timestamp":"{ts}"}}"#,
        );
        let mut existing = fs::read_to_string(path).unwrap_or_default();
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&line);
        existing.push('\n');
        fs::write(path, existing).unwrap();
    }

    fn tmp_tree(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-claude-xfile-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("proj-x")).unwrap();
        dir
    }

    #[test]
    fn cross_file_duplicate_counted_once() {
        // file A and file B both contain (m1, r1). Expected: counted once.
        let root = tmp_tree("once");
        let a = root.join("proj-x").join("a-aaaa.jsonl");
        let b = root.join("proj-x").join("b-bbbb.jsonl");
        write_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&b, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        let data = read(&root);
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(total_out, 100, "duplicate keys must collapse globally");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cross_file_winner_is_later_ts() {
        // Same (m, r) appears with different output and ts in two files. The
        // later ts must win regardless of output magnitude or file ordering.
        let root = tmp_tree("ts");
        let a = root.join("proj-x").join("a-early.jsonl");
        let b = root.join("proj-x").join("b-late.jsonl");
        write_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&b, "m1", "r1", "2026-06-10T12:00:05Z", 50, "/work/x");
        let data = read(&root);
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(total_out, 50, "later ts must win on the tie");
        // The winner stayed in the b session.
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "b-late")
            .unwrap();
        assert_eq!(s.totals.output_tokens, 50);
        assert!(!data.sessions.iter().any(|s| s.session_id == "a-early"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cross_file_tie_breaks_on_output_tokens() {
        // Same ts, different output: larger output wins.
        let root = tmp_tree("output");
        let a = root.join("proj-x").join("a.jsonl");
        let b = root.join("proj-x").join("b.jsonl");
        write_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&b, "m1", "r1", "2026-06-10T12:00:00Z", 250, "/work/x");
        let data = read(&root);
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(total_out, 250);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cross_file_tie_breaks_on_first_path() {
        // Fully identical record in both files; lexicographically first path
        // wins, so the session attributed to a.jsonl survives.
        let root = tmp_tree("path");
        let a = root.join("proj-x").join("a.jsonl");
        let b = root.join("proj-x").join("b.jsonl");
        write_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&b, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        let data = read(&root);
        let surviving: Vec<&str> = data
            .sessions
            .iter()
            .map(|s| s.session_id.as_str())
            .collect();
        assert_eq!(surviving, vec!["a"], "lex-first path wins on full tie");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn retention_collects_deduped_requests_in_ts_order() {
        // From the fixture: proj-a's session collapses three streaming
        // snapshots of msg_S into ONE retained request (the last snapshot's
        // numbers) plus msg_X, sorted ascending by ts.
        let data = read(&fixtures_dir());
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "streaming-and-cwd" && s.project_key == "/work/proj-a")
            .expect("proj-a session present");
        assert_eq!(s.requests.len(), 2, "streaming snapshots collapse to one");
        let first = &s.requests[0];
        assert_eq!(first.input_tokens, 10);
        assert_eq!(first.cache_read_tokens, 100);
        assert_eq!(first.cache_write_5m_tokens, 40);
        assert_eq!(first.cache_write_1h_tokens, 60);
        let second = &s.requests[1];
        assert_eq!(second.input_tokens, 5);
        assert_eq!(second.cache_read_tokens, 50);
        assert_eq!(second.write_tokens(), 0);
        assert!(first.ts < second.ts, "requests sorted ascending by ts");
        // Retention sums must agree with the session totals (same winners).
        let sum_input: u64 = s.requests.iter().map(|r| r.input_tokens).sum();
        let sum_read: u64 = s.requests.iter().map(|r| r.cache_read_tokens).sum();
        assert_eq!(sum_input, s.totals.input_tokens);
        assert_eq!(sum_read, s.totals.cache_read_tokens);
    }

    #[test]
    fn retention_sorts_out_of_order_timestamps_within_a_file() {
        // A file whose records appear in reverse time order must still
        // retain requests ascending by ts.
        let root = tmp_tree("ooo-retention");
        let f = root.join("proj-x").join("out-of-order.jsonl");
        write_record(&f, "m2", "r2", "2026-06-10T12:10:00Z", 20, "/work/x");
        write_record(&f, "m1", "r1", "2026-06-10T12:00:00Z", 10, "/work/x");
        let data = read(&root);
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "out-of-order")
            .expect("session present");
        assert_eq!(s.requests.len(), 2);
        assert!(s.requests[0].ts < s.requests[1].ts);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cross_file_resumed_session_shape() {
        // file A has r1, r2; file B (the resume) has copies of r1, r2 plus
        // new r3. Total tokens must equal r1+r2+r3 each counted once. File A
        // contributes zero surviving records after global dedup (the resume
        // is one logical session), and that is acceptable.
        let root = tmp_tree("resume");
        let a = root.join("proj-x").join("a-original.jsonl");
        let b = root.join("proj-x").join("b-resume.jsonl");
        // r1 and r2 in A.
        write_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 10, "/work/x");
        write_record(&a, "m2", "r2", "2026-06-10T12:00:01Z", 20, "/work/x");
        // The resume copies r1 and r2 verbatim (same ts and output, but later
        // by file ordering) and adds r3.
        write_record(&b, "m1", "r1", "2026-06-10T12:00:00Z", 10, "/work/x");
        write_record(&b, "m2", "r2", "2026-06-10T12:00:01Z", 20, "/work/x");
        write_record(&b, "m3", "r3", "2026-06-10T12:00:02Z", 30, "/work/x");
        let data = read(&root);
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(total_out, 60, "r1+r2+r3, each counted once");
        // r1 and r2 are identical across files; lex-first path wins, so
        // a-original keeps both. r3 lands in b-resume.
        let a_sess = data
            .sessions
            .iter()
            .find(|s| s.session_id == "a-original")
            .expect("a-original surviving with r1+r2");
        assert_eq!(a_sess.totals.output_tokens, 30);
        let b_sess = data
            .sessions
            .iter()
            .find(|s| s.session_id == "b-resume")
            .expect("b-resume surviving with r3");
        assert_eq!(b_sess.totals.output_tokens, 30);
        // Per-request retention happens AFTER global dedup: across all
        // sessions the resume scenario retains exactly three requests
        // (r1, r2, r3), never five.
        let total_requests: usize = data.sessions.iter().map(|s| s.requests.len()).sum();
        assert_eq!(
            total_requests, 3,
            "resume copies must not inflate retention"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// The I2 resume-copy scenario with cache traffic on the records: the
    /// retained tuples that feed `cache_analysis` must carry each cache
    /// write and read exactly once even though the resume file duplicates
    /// the originals verbatim.
    #[test]
    fn retention_survives_resume_copy_with_cache_writes() {
        fn write_cache_record(
            path: &Path,
            message_id: &str,
            request_id: &str,
            ts: &str,
            read: u64,
            w5: u64,
            w1: u64,
        ) {
            let line = format!(
                concat!(
                    r#"{{"message":{{"model":"claude-sonnet-4-6","id":"{message_id}","usage":"#,
                    r#"{{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":{read},"#,
                    r#""cache_creation_input_tokens":{creation},"cache_creation":"#,
                    r#"{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}}}}}},"#,
                    r#""requestId":"{request_id}","cwd":"/work/x","timestamp":"{ts}"}}"#,
                ),
                message_id = message_id,
                request_id = request_id,
                ts = ts,
                read = read,
                creation = w5 + w1,
                w5 = w5,
                w1 = w1,
            );
            let mut existing = fs::read_to_string(path).unwrap_or_default();
            if !existing.is_empty() && !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&line);
            existing.push('\n');
            fs::write(path, existing).unwrap();
        }

        let root = tmp_tree("resume-cache");
        let a = root.join("proj-x").join("a-original.jsonl");
        let b = root.join("proj-x").join("b-resume.jsonl");
        // Original: r1 writes the prefix (1h), r2 reads it.
        write_cache_record(&a, "m1", "r1", "2026-06-10T12:00:00Z", 0, 0, 9000);
        write_cache_record(&a, "m2", "r2", "2026-06-10T12:01:00Z", 9000, 0, 0);
        // Resume copies r1 and r2 verbatim, then adds r3 (another read).
        write_cache_record(&b, "m1", "r1", "2026-06-10T12:00:00Z", 0, 0, 9000);
        write_cache_record(&b, "m2", "r2", "2026-06-10T12:01:00Z", 9000, 0, 0);
        write_cache_record(&b, "m3", "r3", "2026-06-10T12:02:00Z", 9000, 0, 0);
        let data = read(&root);
        let retained_w1: u64 = data
            .sessions
            .iter()
            .flat_map(|s| s.requests.iter())
            .map(|r| r.cache_write_1h_tokens)
            .sum();
        let retained_reads: u64 = data
            .sessions
            .iter()
            .flat_map(|s| s.requests.iter())
            .map(|r| r.cache_read_tokens)
            .sum();
        let retained_count: usize = data.sessions.iter().map(|s| s.requests.len()).sum();
        assert_eq!(retained_count, 3, "r1, r2, r3 each retained once");
        assert_eq!(retained_w1, 9000, "the copied write counts once");
        assert_eq!(retained_reads, 18_000, "r2 and r3 reads count once each");
        let _ = fs::remove_dir_all(&root);
    }

    /// Adversarial pin (A7 review): the same (message_id, request_id) appears
    /// in two files with DIFFERENT cache usage numbers. The winner rule
    /// (later ts, then larger output, then lex-first file) decides which
    /// record survives; per-request retention must take the WINNER's cache
    /// tuple, not the loser's. A naive aggregator that picked the first
    /// record it saw would carry the wrong tuple downstream into churn,
    /// gap, and dollar math.
    #[test]
    fn cross_file_retention_keeps_winners_cache_tuple_not_losers() {
        struct CacheFields {
            output: u64,
            read: u64,
            w5: u64,
            w1: u64,
        }
        fn write_cache_record(
            path: &Path,
            message_id: &str,
            request_id: &str,
            ts: &str,
            fields: &CacheFields,
        ) {
            let line = format!(
                concat!(
                    r#"{{"message":{{"model":"claude-sonnet-4-6","id":"{message_id}","usage":"#,
                    r#"{{"input_tokens":1,"output_tokens":{output},"cache_read_input_tokens":{read},"#,
                    r#""cache_creation_input_tokens":{creation},"cache_creation":"#,
                    r#"{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}}}}}},"#,
                    r#""requestId":"{request_id}","cwd":"/work/x","timestamp":"{ts}"}}"#,
                ),
                message_id = message_id,
                request_id = request_id,
                ts = ts,
                output = fields.output,
                read = fields.read,
                creation = fields.w5 + fields.w1,
                w5 = fields.w5,
                w1 = fields.w1,
            );
            let mut existing = fs::read_to_string(path).unwrap_or_default();
            if !existing.is_empty() && !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(&line);
            existing.push('\n');
            fs::write(path, existing).unwrap();
        }

        let root = tmp_tree("xfile-diff-cache");
        let a = root.join("proj-x").join("a-loser.jsonl");
        let b = root.join("proj-x").join("b-winner.jsonl");
        // Same (m1, r1) appears in both files with the SAME ts; b has more
        // output, so b wins on the second tier of the winner rule. The
        // cache numbers also differ: a says read=1000, w1=2000; b says
        // read=3000, w1=7000. The winner is b, so retention must be 3000
        // and 7000, not 1000 and 2000.
        write_cache_record(
            &a,
            "m1",
            "r1",
            "2026-06-10T12:00:00Z",
            &CacheFields {
                output: 50,
                read: 1000,
                w5: 0,
                w1: 2000,
            },
        );
        write_cache_record(
            &b,
            "m1",
            "r1",
            "2026-06-10T12:00:00Z",
            &CacheFields {
                output: 500,
                read: 3000,
                w5: 0,
                w1: 7000,
            },
        );
        let data = read(&root);
        let retained_w1: u64 = data
            .sessions
            .iter()
            .flat_map(|s| s.requests.iter())
            .map(|r| r.cache_write_1h_tokens)
            .sum();
        let retained_reads: u64 = data
            .sessions
            .iter()
            .flat_map(|s| s.requests.iter())
            .map(|r| r.cache_read_tokens)
            .sum();
        let retained_count: usize = data.sessions.iter().map(|s| s.requests.len()).sum();
        assert_eq!(retained_count, 1, "one logical record after dedup");
        assert_eq!(
            retained_w1, 7000,
            "winner's cache_write_1h must be retained, not loser's"
        );
        assert_eq!(
            retained_reads, 3000,
            "winner's cache_read must be retained, not loser's"
        );
        // The winning session is b-winner; the totals also match the winner.
        let winner_session = data
            .sessions
            .iter()
            .find(|s| s.session_id == "b-winner")
            .expect("b-winner survives the merge");
        assert_eq!(winner_session.totals.cache_read_tokens, 3000);
        assert_eq!(winner_session.totals.cache_write_1h_tokens, 7000);
        let _ = fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------
    // Subagent stream discovery and attribution.
    //
    // Fixture layout under `fixtures/usage/claude-code/`:
    //   proj-a/streaming-and-cwd.jsonl    (original fixture, no subagents)
    //   proj-b/parent-session.jsonl       (parent jsonl)
    //   proj-b/parent-session/subagents/agent-aaa.jsonl   (subagent, same cwd)
    //   proj-b/parent-session/subagents/agent-aaa.meta.json  (companion;
    //                                                         must be ignored)
    //   proj-b/parent-session/subagents/agent-bbb.jsonl   (subagent, different cwd)
    //   proj-b-empty/parent-empty.jsonl  (parent jsonl)
    //   proj-b-empty/parent-empty/subagents/  (empty dir)
    // ------------------------------------------------------------------

    #[test]
    fn subagent_streams_emit_their_own_session_rows() {
        let data = read(&fixtures_dir());
        // The parent session row lives where it always did: same cwd as the
        // parent jsonl's records, session_id = file stem.
        let parent = data
            .sessions
            .iter()
            .find(|s| s.session_id == "parent-session" && s.project_key == "/work/proj-b")
            .expect("parent-session row present");
        // Parent records: msg_P1 (output 50) + msg_P2 (output 30).
        assert_eq!(parent.totals.output_tokens, 80);
        // The agent-aaa row is its own SessionUsage and lands on /work/proj-b
        // because the record's own cwd matched the parent's. We attribute by
        // per-record cwd, not by reversing the slug.
        let sub_a = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-aaa" && s.project_key == "/work/proj-b")
            .expect("agent-aaa row present");
        assert_eq!(sub_a.totals.output_tokens, 35);
        assert_eq!(sub_a.totals.input_tokens, 5 + 3);
        // The agent-bbb row carries the subagent's OWN cwd (/work/proj-b-deep),
        // not the parent's. This is the key attribution rule: per-record cwd.
        let sub_b = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-bbb" && s.project_key == "/work/proj-b-deep")
            .expect("agent-bbb row present on its own cwd");
        assert_eq!(sub_b.totals.output_tokens, 25);
        // The side-table identifies them as subagents and points to the
        // originating parent jsonl stem.
        assert!(data.is_subagent(sub_a));
        assert!(data.is_subagent(sub_b));
        assert!(!data.is_subagent(parent));
        assert_eq!(
            data.subagent_parent_ids
                .get(&(sub_a.session_id.clone(), sub_a.project_key.clone())),
            Some(&"parent-session".to_string()),
        );
        assert_eq!(
            data.subagent_parent_ids
                .get(&(sub_b.session_id.clone(), sub_b.project_key.clone())),
            Some(&"parent-session".to_string()),
        );
    }

    #[test]
    fn subagent_stream_count_distinguishes_from_parent_count() {
        let data = read(&fixtures_dir());
        // Fixture parent session rows:
        //   proj-a/streaming-and-cwd.jsonl emits TWO rows (one per surviving
        //     cwd: /work/proj-a and /work/proj-b),
        //   proj-b/parent-session.jsonl emits one row,
        //   proj-b-empty/parent-empty.jsonl emits one row.
        // Subagent rows: agent-aaa on /work/proj-b, agent-bbb on /work/proj-b-deep.
        assert_eq!(data.parent_session_count(), 4);
        assert_eq!(data.subagent_stream_count(), 2);
        // Total sessions vector includes both kinds.
        assert_eq!(data.sessions.len(), 6);
    }

    #[test]
    fn subagent_meta_json_files_are_ignored_in_walk() {
        // The fixture has agent-aaa.meta.json; it must not become a session.
        let data = read(&fixtures_dir());
        let any_meta = data
            .sessions
            .iter()
            .any(|s| s.session_id.ends_with(".meta") || s.session_id == "agent-aaa.meta");
        assert!(!any_meta, "meta.json must not show up as a session row");
    }

    #[test]
    fn empty_subagents_dir_does_not_break_ingestion() {
        // proj-b-empty has an empty `parent-empty/subagents/` dir; the parent
        // jsonl still ingests and no subagent rows are added for that parent.
        let data = read(&fixtures_dir());
        let parent = data
            .sessions
            .iter()
            .find(|s| s.session_id == "parent-empty" && s.project_key == "/work/proj-b-empty")
            .expect("parent-empty present");
        assert_eq!(parent.totals.output_tokens, 13);
        // No subagent row points to parent-empty.
        let any_pointer = data
            .subagent_parent_ids
            .values()
            .any(|p| p == "parent-empty");
        assert!(
            !any_pointer,
            "empty subagents dir must not register any stream"
        );
    }

    #[test]
    fn subagent_retention_populated_for_cache_analysis() {
        let data = read(&fixtures_dir());
        let sub_a = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-aaa")
            .expect("agent-aaa present");
        // Two retained requests for agent-aaa (msg_A1 with a write, msg_A2
        // with a read). Sorted by ts ascending.
        assert_eq!(sub_a.requests.len(), 2);
        assert!(sub_a.requests[0].ts < sub_a.requests[1].ts);
        // The first request's cache_write_5m equals 500 (the prefix proxy);
        // the second's cache_read equals 500.
        assert_eq!(sub_a.requests[0].cache_write_5m_tokens, 500);
        assert_eq!(sub_a.requests[1].cache_read_tokens, 500);
    }

    #[test]
    fn parent_session_id_from_path_extracts_correctly() {
        // The classification rule: a path whose immediate parent dir is
        // `subagents/` belongs to the parent session named by the
        // grandparent directory.
        let p =
            PathBuf::from("/x/.claude/projects/-project/parent-session/subagents/agent-aaa.jsonl");
        assert_eq!(
            parent_session_id_from_path(&p),
            Some("parent-session".to_string()),
        );
        // Parent jsonl: NOT a subagent.
        let parent = PathBuf::from("/x/.claude/projects/-project/parent-session.jsonl");
        assert_eq!(parent_session_id_from_path(&parent), None);
        // Unrelated nested layout: not classified as subagent.
        let other = PathBuf::from("/x/.claude/projects/-project/something/else/file.jsonl");
        assert_eq!(parent_session_id_from_path(&other), None);
    }

    #[test]
    fn is_subagent_jsonl_recognizes_only_agent_jsonl() {
        assert!(is_subagent_jsonl(&PathBuf::from("agent-abc.jsonl")));
        assert!(is_subagent_jsonl(&PathBuf::from("/x/agent-abc123.jsonl")));
        assert!(!is_subagent_jsonl(&PathBuf::from("agent-abc.meta.json")));
        assert!(!is_subagent_jsonl(&PathBuf::from("agent-abc.txt")));
        // Files inside subagents/ that don't start with `agent-` are not ours.
        assert!(!is_subagent_jsonl(&PathBuf::from("session.jsonl")));
        assert!(!is_subagent_jsonl(&PathBuf::from("agent.jsonl")));
    }

    /// Synthetic collision: pin the dedup rule across the parent/subagent
    /// boundary. Empirically (47 sessions, 299 files, 6.3K subagent keys, 9K
    /// parent keys observed on this workstation) the two domains are
    /// strictly disjoint on (message_id, request_id). But if a future Claude
    /// Code release ever DID duplicate, the global last-wins dedup must
    /// still collapse safely under the existing winner rule. This test
    /// forces a collision and asserts the count.
    #[test]
    fn cross_parent_subagent_dedup_collapses_collisions() {
        let root = tmp_tree("parent-vs-subagent");
        let proj = root.join("proj-x");
        let parent = proj.join("parent-X.jsonl");
        let sub_dir = proj.join("parent-X").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-yyy.jsonl");
        // Parent and subagent BOTH carry (m1, r1). The subagent has a later
        // timestamp, so it wins on the first tier of the winner rule.
        write_record(&parent, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&sub, "m1", "r1", "2026-06-10T12:00:01Z", 200, "/work/x");
        let data = read(&root);
        // Exactly one logical record after dedup.
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(total_out, 200, "collision must collapse to one winner");
        let total_records: usize = data.sessions.iter().map(|s| s.requests.len()).sum();
        assert_eq!(total_records, 1, "retention must reflect the single winner");
        // The winner (later ts) was in the subagent file, so a subagent row
        // exists with that record and the parent row may be absent.
        let sub_session = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-yyy")
            .expect("subagent winner present");
        assert_eq!(sub_session.totals.output_tokens, 200);
        let _ = fs::remove_dir_all(&root);
    }

    /// Wave 2 review pin: when a parent-vs-subagent collision happens AND
    /// the PARENT record wins (later ts than the subagent's copy), the
    /// subagent file must not register an orphan side-table entry. The
    /// existing `cross_parent_subagent_dedup_collapses_collisions` covers
    /// the subagent-wins direction; this is the symmetric case the review
    /// flagged. The grouped map is keyed on the winner's `is_subagent`
    /// flag, so a parent-winning row stays parent and the side-table is
    /// populated solely from grouped entries that actually exist.
    #[test]
    fn parent_winning_collision_does_not_register_orphan_subagent_side_table() {
        let root = tmp_tree("parent-wins-collision");
        let proj = root.join("proj-x");
        let parent = proj.join("parent-X.jsonl");
        let sub_dir = proj.join("parent-X").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-yyy.jsonl");
        // Parent has the later ts and wins on the first tier of the rule.
        // The subagent file contains only this colliding record, so after
        // global dedup the subagent has no surviving records and emits no
        // SessionUsage row; the side-tables must reflect that absence.
        write_record(&parent, "m1", "r1", "2026-06-10T12:00:01Z", 200, "/work/x");
        write_record(&sub, "m1", "r1", "2026-06-10T12:00:00Z", 100, "/work/x");
        let data = read(&root);
        let total_out: u64 = data.sessions.iter().map(|s| s.totals.output_tokens).sum();
        assert_eq!(
            total_out, 200,
            "collision must collapse to the parent winner"
        );
        // The parent session is present and classified as parent.
        let parent_session = data
            .sessions
            .iter()
            .find(|s| s.session_id == "parent-X")
            .expect("parent winner present");
        assert!(!data.is_subagent(parent_session));
        // No subagent row exists for agent-yyy. The side-tables must be
        // empty: no orphan key, no orphan parent-pointer.
        assert!(data.sessions.iter().all(|s| s.session_id != "agent-yyy"));
        assert!(
            data.subagent_session_keys.is_empty(),
            "side-table must not carry an orphan entry: {:?}",
            data.subagent_session_keys
        );
        assert!(
            data.subagent_parent_ids.is_empty(),
            "parent-pointer map must not carry an orphan entry: {:?}",
            data.subagent_parent_ids
        );
        // Headlines: one parent session, zero subagent streams.
        assert_eq!(data.parent_session_count(), 1);
        assert_eq!(data.subagent_stream_count(), 0);
        let _ = fs::remove_dir_all(&root);
    }

    /// Wave 2 review pin: a `<parent>/subagents/` directory whose parent
    /// jsonl is missing (orphan subagents). The subagent rows must still
    /// ingest and tag the missing parent as their parent_session_id.
    #[test]
    fn orphan_subagent_dir_with_missing_parent_still_ingests() {
        let root = tmp_tree("orphan-subagents");
        let proj = root.join("proj-x");
        // No parent-Z.jsonl, only the subagents directory.
        let sub_dir = proj.join("parent-Z").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-orphan.jsonl");
        write_record(&sub, "mO", "rO", "2026-06-10T12:00:00Z", 77, "/work/x");
        let data = read(&root);
        let s = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-orphan")
            .expect("subagent row present even with no parent jsonl");
        assert_eq!(s.totals.output_tokens, 77);
        assert!(data.is_subagent(s));
        assert_eq!(
            data.subagent_parent_ids
                .get(&(s.session_id.clone(), s.project_key.clone())),
            Some(&"parent-Z".to_string()),
            "subagent attribution still points at the missing parent stem",
        );
        // Headlines: zero parents, one subagent stream.
        assert_eq!(data.parent_session_count(), 0);
        assert_eq!(data.subagent_stream_count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    /// Wave 2 review pin: a subagent file whose every record is filtered
    /// out by `extract_record` (no usage-bearing field, synthetic model,
    /// missing keys) must produce no SessionUsage row and no side-table
    /// entry; the file is observed but its absence is silent (zero is
    /// not an error here).
    #[test]
    fn subagent_with_zero_usage_bearing_records_emits_nothing() {
        let root = tmp_tree("zero-usage-subagent");
        let proj = root.join("proj-x");
        let parent = proj.join("parent-Q.jsonl");
        let sub_dir = proj.join("parent-Q").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-quiet.jsonl");
        // The parent has a real record so the project shows up at all.
        write_record(&parent, "mp", "rp", "2026-06-10T12:00:00Z", 50, "/work/x");
        // The subagent contains a synthetic-model line and a malformed
        // line; neither survives extract_record.
        let lines = "\
{\"message\":{\"model\":\"<synthetic>\",\"id\":\"ms\",\"usage\":{\"input_tokens\":0,\"output_tokens\":999,\"cache_read_input_tokens\":0,\"cache_creation_input_tokens\":0}},\"requestId\":\"rs\",\"cwd\":\"/work/x\",\"timestamp\":\"2026-06-10T12:00:00Z\"}\nnot json at all\n";
        fs::write(&sub, lines).unwrap();
        let data = read(&root);
        assert!(data.sessions.iter().all(|s| s.session_id != "agent-quiet"));
        assert!(data.subagent_session_keys.is_empty());
        assert!(data.subagent_parent_ids.is_empty());
        assert_eq!(data.subagent_stream_count(), 0);
        // The synthetic line wasn't a JSON parse error; only the second
        // bare-text line counts toward skipped_lines.
        assert!(data.skipped_lines >= 1);
        let _ = fs::remove_dir_all(&root);
    }

    /// Synthetic disjointness: pin the practically-observed shape that
    /// parent and subagent keys do NOT overlap, by checking that two
    /// distinct keys remain distinct after global dedup. This is the
    /// "fixture" version of the workstation probe documented in the
    /// module rustdoc.
    #[test]
    fn subagent_records_disjoint_from_parent_in_practice() {
        let root = tmp_tree("disjoint");
        let proj = root.join("proj-x");
        let parent = proj.join("disjoint-parent.jsonl");
        let sub_dir = proj.join("disjoint-parent").join("subagents");
        fs::create_dir_all(&sub_dir).unwrap();
        let sub = sub_dir.join("agent-zzz.jsonl");
        write_record(&parent, "mp", "rp", "2026-06-10T12:00:00Z", 100, "/work/x");
        write_record(&sub, "ms", "rs", "2026-06-10T12:01:00Z", 200, "/work/x");
        let data = read(&root);
        // Both rows survive: one parent, one subagent.
        let parent_session = data
            .sessions
            .iter()
            .find(|s| s.session_id == "disjoint-parent")
            .expect("parent row");
        let sub_session = data
            .sessions
            .iter()
            .find(|s| s.session_id == "agent-zzz")
            .expect("subagent row");
        assert_eq!(parent_session.totals.output_tokens, 100);
        assert_eq!(sub_session.totals.output_tokens, 200);
        assert!(!data.is_subagent(parent_session));
        assert!(data.is_subagent(sub_session));
        let _ = fs::remove_dir_all(&root);
    }

    /// Smoke run against the developer's actual local logs. Ignored by default
    /// because the paths only exist on a workstation; invoke with
    /// `cargo test --bin tolkin --release smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn smoke_local_dirs_report_numbers() {
        let home = match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h),
            None => return,
        };
        let (claude_dir, codex_dir) = super::super::default_dirs(&home);

        // Stage one across every Claude file, with a side count of how many
        // (message_id, request_id) keys were collapsed across files.
        let mut per_file: Vec<(PathBuf, Vec<DedupedRecord>)> = Vec::new();
        let mut skipped_lines = 0u64;
        let mut skipped_files = 0u64;
        if claude_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&claude_dir) {
                for entry in entries.flatten() {
                    let pp = entry.path();
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
                        let records =
                            parse_file_records(&path, &mut skipped_lines, &mut skipped_files);
                        per_file.push((path, records));
                    }
                }
            }
        }
        let stage_one_records: u64 = per_file.iter().map(|(_, r)| r.len() as u64).sum();

        let mut data = UsageData {
            skipped_lines,
            skipped_files,
            ..UsageData::default()
        };
        aggregate_records(per_file, &mut data);
        let stage_two_records: u64 = data
            .sessions
            .iter()
            .map(|s| s.by_day.values().map(|_| 1u64).sum::<u64>())
            .sum::<u64>(); // not a record count; computed below from totals
                           // Real cross-file collapses count: stage_one - winners-kept. winners
                           // count == sum over sessions of len(by_day) is wrong (records can
                           // share a day). Recompute against the actual collapse: the winners
                           // map size equals the unique-keys count, which is what landed in
                           // sessions in aggregate; we recompute by re-merging without grouping.
        let _ = stage_two_records;

        let claude = data;
        let codex = super::super::codex::read(&codex_dir);

        // Recompute the collapse count by replaying stage one and counting
        // unique keys directly; cheaper than threading a side-channel through
        // aggregate_records. We do it inline to keep the probe self-contained.
        let mut keys: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut total_records = 0u64;
        if claude_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&claude_dir) {
                for entry in entries.flatten() {
                    let pp = entry.path();
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
                        let mut sl = 0u64;
                        let mut sf = 0u64;
                        let recs = parse_file_records(&path, &mut sl, &mut sf);
                        for r in recs {
                            total_records += 1;
                            *keys.entry((r.message_id, r.request_id)).or_default() += 1;
                        }
                    }
                }
            }
        }
        let unique_keys = keys.len() as u64;
        let collapsed_dup_keys = keys.values().filter(|v| **v > 1).count() as u64;
        let cross_file_collapses = total_records.saturating_sub(unique_keys);

        let total_in: u64 = claude
            .sessions
            .iter()
            .chain(codex.sessions.iter())
            .map(|s| s.totals.input_tokens)
            .sum();
        let total_out: u64 = claude
            .sessions
            .iter()
            .chain(codex.sessions.iter())
            .map(|s| s.totals.output_tokens)
            .sum();
        let total_cache_read: u64 = claude
            .sessions
            .iter()
            .chain(codex.sessions.iter())
            .map(|s| s.totals.cache_read_tokens)
            .sum();
        let total_cache_write_5m: u64 = claude
            .sessions
            .iter()
            .chain(codex.sessions.iter())
            .map(|s| s.totals.cache_write_5m_tokens)
            .sum();
        let total_cache_write_1h: u64 = claude
            .sessions
            .iter()
            .chain(codex.sessions.iter())
            .map(|s| s.totals.cache_write_1h_tokens)
            .sum();
        let total_cache_write = total_cache_write_5m + total_cache_write_1h;

        let mut model_counts: BTreeMap<String, u64> = BTreeMap::new();
        for s in claude.sessions.iter().chain(codex.sessions.iter()) {
            for (m, t) in &s.by_model {
                *model_counts.entry(m.clone()).or_default() += t.input_side() + t.output_tokens;
            }
        }
        let mut top: Vec<(String, u64)> = model_counts.into_iter().collect();
        top.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        eprintln!("--- smoke local dirs ---");
        eprintln!("claude sessions emitted: {}", claude.sessions.len());
        eprintln!("codex sessions parsed: {}", codex.sessions.len());
        eprintln!(
            "skipped lines: claude={} codex={}",
            claude.skipped_lines, codex.skipped_lines
        );
        eprintln!(
            "skipped files: claude={} codex={}",
            claude.skipped_files, codex.skipped_files
        );
        eprintln!("claude stage-one records: {stage_one_records}");
        eprintln!("claude unique (message_id, request_id) keys: {unique_keys}");
        eprintln!("claude cross-file duplicate keys collapsed: {collapsed_dup_keys}");
        eprintln!(
            "claude cross-file record collapses (records - unique keys): {cross_file_collapses}"
        );
        eprintln!("total input tokens: {total_in}");
        eprintln!("total output tokens: {total_out}");
        eprintln!("total cache_read tokens: {total_cache_read}");
        eprintln!("total cache_write tokens (5m+1h): {total_cache_write}");
        eprintln!("  cache_write_5m: {total_cache_write_5m}");
        eprintln!("  cache_write_1h: {total_cache_write_1h}");
        eprintln!("top model ids:");
        for (m, c) in top.iter().take(8) {
            eprintln!("  {m}: {c}");
        }
    }
}
