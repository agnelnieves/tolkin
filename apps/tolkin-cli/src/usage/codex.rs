//! Codex CLI rollout reader.
//!
//! Layout: `<sessions_dir>/YYYY/MM/DD/rollout-*.jsonl`. Each rollout opens with
//! a `session_meta` line that carries `cwd` and `id`, followed by a
//! `turn_context` line that carries `model`. Token spend lives in event_msg
//! payloads of type `token_count` whose `info` is either NULL (skip) or carries
//! `total_token_usage` (cumulative) and `last_token_usage` (this turn's delta).
//!
//! Totals strategy: we sum the per-event `last_token_usage` values rather than
//! reading only the final `total_token_usage`. Both are mathematically
//! identical on a clean session because `total_token_usage` IS the running sum,
//! but the per-event reading is what lets us bucket each turn into its own UTC
//! day, which `total_token_usage` alone cannot do. The test
//! `codex_invariant_total_matches_sum_of_last` enforces the equality.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::claude_code::{parse_iso8601, utc_day_from_secs};
use super::types::{SessionUsage, UsageData, UsageSource, UsageTotals};

/// Walks `<sessions_dir>/YYYY/MM/DD/rollout-*.jsonl`. Missing or unreadable dir
/// produces an empty UsageData.
#[allow(dead_code)] // direct no-cache entry, exercised by tests; production path is cache::read_all_cached
pub fn read(sessions_dir: &Path) -> UsageData {
    let mut data = UsageData::default();
    if !sessions_dir.is_dir() {
        return data;
    }
    // Three-level walk: YYYY/MM/DD. We tolerate stray files at every level
    // (some users have notes alongside the date tree).
    let Ok(years) = fs::read_dir(sessions_dir) else {
        return data;
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
                    read_file_into(&path, &mut data);
                }
            }
        }
    }
    data
}

/// Parse one rollout into a SessionUsage. Public so the cache layer can reuse
/// the parser without re-walking directories.
#[allow(dead_code)] // kept beside read(); the cache layer calls parse_file directly
pub fn read_file_into(path: &Path, data: &mut UsageData) {
    if let Some(session) = parse_file(path, &mut data.skipped_lines, &mut data.skipped_files) {
        data.sessions.push(session);
    }
}

/// Returns None when the file is unreadable, has no usage events, or carries
/// no `cwd` it can be attributed to. A rollout that has usage events but no
/// session-meta `cwd` is counted in `skipped_files` rather than dropped
/// silently. Mid-session model swaps are NOT tracked: all tokens get
/// attributed to the model captured at the opening turn_context. The
/// alternative would require per-event model tracking, which the rollout
/// schema does not promise to support stably across Codex versions.
pub fn parse_file(
    path: &Path,
    skipped_lines: &mut u64,
    skipped_files: &mut u64,
) -> Option<SessionUsage> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            *skipped_files += 1;
            return None;
        }
    };

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut totals = UsageTotals::default();
    let mut by_day: BTreeMap<String, UsageTotals> = BTreeMap::new();
    let mut first_ts: Option<u64> = None;
    let mut last_ts: u64 = 0;
    let mut saw_event = false;

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

        let line_ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if line_ty == "session_meta" {
            let payload = v.get("payload");
            if let Some(p) = payload {
                if cwd.is_none() {
                    cwd = p.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
                }
                if session_id.is_none() {
                    session_id = p.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
                }
                // session_meta sometimes carries a model under a nested object
                // depending on the codex version; harmless when absent.
                if model.is_none() {
                    model = p
                        .get("model")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string());
                }
            }
            continue;
        }
        if line_ty == "turn_context" {
            // First turn_context wins for the model attribution. See the
            // function rustdoc for the mid-session-swap rationale.
            if model.is_none() {
                model = v
                    .get("payload")
                    .and_then(|p| p.get("model"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());
            }
            continue;
        }
        if line_ty != "event_msg" {
            continue;
        }
        let Some(payload) = v.get("payload") else {
            continue;
        };
        let payload_ty = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if payload_ty != "token_count" {
            continue;
        }
        let Some(info) = payload.get("info") else {
            continue;
        };
        if info.is_null() {
            continue;
        }
        let Some(last) = info.get("last_token_usage") else {
            continue;
        };
        let delta = extract_delta(last);
        if delta.is_empty() {
            continue;
        }
        let ts_str = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
        let Some(ts) = parse_iso8601(ts_str) else {
            *skipped_lines += 1;
            continue;
        };
        let day = utc_day_from_secs(ts);
        totals.add(&delta);
        by_day.entry(day).or_default().add(&delta);
        first_ts.get_or_insert(ts);
        if ts > last_ts {
            last_ts = ts;
        }
        saw_event = true;
    }

    if !saw_event {
        return None;
    }
    // A rollout with usage events but no session-meta cwd cannot be
    // attributed to any project. Count it in skipped_files so the operator
    // sees that ingestion noticed something it could not place; dropping it
    // silently used to hide a real signal.
    let cwd = match cwd {
        Some(c) => c,
        None => {
            *skipped_files += 1;
            return None;
        }
    };
    let session_id = session_id.unwrap_or_else(|| {
        // Fall back to the rollout id embedded in the filename if meta missing.
        // Filename shape: rollout-YYYY-MM-DDTHH-MM-SS-<uuid>.jsonl
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        stem.rsplit('-')
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| stem.to_string())
    });
    let model = model.unwrap_or_else(|| "codex-unknown".to_string());

    let mut by_model = BTreeMap::new();
    by_model.insert(model, totals);

    Some(SessionUsage {
        source: UsageSource::Codex,
        session_id,
        project_key: cwd,
        first_ts: first_ts.unwrap_or(0),
        last_ts,
        totals,
        by_model,
        by_day,
    })
}

/// Map a Codex `last_token_usage` block to the shared UsageTotals shape.
/// `input_tokens` INCLUDES `cached_input_tokens` per Codex semantics, so the
/// "fresh" input we charge against the input rate is the saturating diff.
/// `output_tokens` already includes `reasoning_output_tokens`; we keep it.
fn extract_delta(last: &Value) -> UsageTotals {
    let input_with_cache = last
        .get("input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cached = last
        .get("cached_input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let output = last
        .get("output_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    UsageTotals {
        input_tokens: input_with_cache.saturating_sub(cached),
        output_tokens: output,
        cache_read_tokens: cached,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
    }
}

/// Returns the final `total_token_usage` block for a rollout, used by the
/// invariant test to confirm that `sum(last) == total`. Not used by the
/// production reader.
#[cfg(test)]
fn final_total(path: &Path) -> Option<UsageTotals> {
    let text = fs::read_to_string(path).ok()?;
    let mut latest: Option<UsageTotals> = None;
    for line in text.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let info = v
            .get("payload")
            .and_then(|p| (p.get("type")?.as_str()? == "token_count").then_some(p))
            .and_then(|p| p.get("info"))
            .filter(|i| !i.is_null());
        let Some(info) = info else { continue };
        let Some(total) = info.get("total_token_usage") else {
            continue;
        };
        latest = Some(extract_delta(total));
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("usage")
            .join("codex")
    }

    fn rollout_path() -> PathBuf {
        fixtures_dir()
            .join("2026")
            .join("06")
            .join("10")
            .join("rollout-2026-06-10T12-00-00-feedfeed.jsonl")
    }

    #[test]
    fn empty_dir_produces_empty_usage_data() {
        let dir = std::env::temp_dir().join(format!("tolkin-codex-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = read(&dir);
        assert!(data.sessions.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn codex_reads_cwd_model_and_totals() {
        let data = read(&fixtures_dir());
        assert_eq!(data.sessions.len(), 1, "one rollout, one session");
        let s = &data.sessions[0];
        assert_eq!(s.project_key, "/work/codex-proj");
        // From turn_context.
        assert!(s.by_model.contains_key("gpt-5.5"));
        // Two real token_count events:
        //   first: input 100, cached 60, output 20  -> input_tokens 40, cache_read 60
        //   second: input 200, cached 150, output 30 -> input_tokens 50, cache_read 150
        assert_eq!(s.totals.input_tokens, 90);
        assert_eq!(s.totals.cache_read_tokens, 210);
        assert_eq!(s.totals.output_tokens, 50);
    }

    #[test]
    fn codex_invariant_total_matches_sum_of_last() {
        // The contract we rely on: at the end of a clean rollout, the final
        // total_token_usage equals the sum of all last_token_usage values.
        let mut skipped_lines = 0;
        let mut skipped_files = 0;
        let session = parse_file(&rollout_path(), &mut skipped_lines, &mut skipped_files).unwrap();
        let final_t = final_total(&rollout_path()).unwrap();
        assert_eq!(
            session.totals.input_tokens + session.totals.cache_read_tokens,
            final_t.input_tokens + final_t.cache_read_tokens
        );
        assert_eq!(session.totals.output_tokens, final_t.output_tokens);
    }

    #[test]
    fn codex_skips_info_null_events() {
        // The fixture has three token_count events: two with info, one info:null.
        // The null one must contribute zero (the bucket sums must match the
        // explicit deltas exactly).
        let data = read(&fixtures_dir());
        let s = &data.sessions[0];
        assert_eq!(s.totals.output_tokens, 50);
    }

    #[test]
    fn codex_buckets_by_day_per_event() {
        let data = read(&fixtures_dir());
        let s = &data.sessions[0];
        // The fixture places one event on 2026-06-10 and one on 2026-06-11.
        assert!(s.by_day.contains_key("2026-06-10"));
        assert!(s.by_day.contains_key("2026-06-11"));
    }

    #[test]
    fn codex_missing_cwd_increments_skipped_files() {
        // Build a synthetic rollout: usage events present, but the
        // session_meta line carries no cwd. Expected: parse_file returns
        // None AND increments skipped_files (not silently dropped).
        let dir = std::env::temp_dir().join(format!("tolkin-codex-no-cwd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout-2026-06-10T12-00-00-deadbeef.jsonl");
        let lines = [
            r#"{"timestamp":"2026-06-10T12:00:00.000Z","type":"session_meta","payload":{"id":"no-cwd","timestamp":"2026-06-10T12:00:00.000Z","originator":"Codex","cli_version":"0.0.0","source":"fixture"}}"#,
            r#"{"timestamp":"2026-06-10T12:00:01.000Z","type":"turn_context","payload":{"turn_id":"t1","model":"gpt-5.5","approval_policy":"never"}}"#,
            r#"{"timestamp":"2026-06-10T12:00:10.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15},"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":15},"model_context_window":200000}}}"#,
        ];
        fs::write(&path, lines.join("\n")).unwrap();
        let mut skipped_lines = 0u64;
        let mut skipped_files = 0u64;
        let result = parse_file(&path, &mut skipped_lines, &mut skipped_files);
        assert!(result.is_none(), "no cwd means no SessionUsage emitted");
        assert_eq!(skipped_files, 1, "missing cwd must increment skipped_files");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Walks real local Codex rollouts and reports the divergence between
    /// `sum(last_token_usage)` and the final `total_token_usage`. On the
    /// fixture these are identical (and `codex_invariant_total_matches_sum_of_last`
    /// asserts that). On long real sessions they diverge: `total_token_usage`
    /// is a server-reported figure that does not include certain
    /// not-charged-back turns (compaction snapshots, retries), while
    /// `sum(last)` counts everything the user's machine emitted. We chose the
    /// sum strategy precisely because it lets us bucket by day, and the
    /// divergence is upward-bounded (sum >= final) which is the safer side
    /// when surfacing "what your session sent". Ignored by default; invoke
    /// with `-- --ignored --nocapture` for a one-shot probe.
    #[test]
    #[ignore]
    fn codex_invariant_probe_on_local_rollouts() {
        let home = match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h),
            None => return,
        };
        let dir = home.join(".codex").join("sessions");
        if !dir.is_dir() {
            return;
        }
        let mut checked = 0u64;
        let mut sum_ge_final = 0u64;
        let mut max_delta = 0u64;
        walk_rollouts(&dir, &mut |path: &Path| {
            let mut sl = 0u64;
            let mut sf = 0u64;
            let session = match parse_file(path, &mut sl, &mut sf) {
                Some(s) => s,
                None => return,
            };
            let Some(final_t) = final_total(path) else {
                return;
            };
            checked += 1;
            let sum_in = session.totals.input_tokens + session.totals.cache_read_tokens;
            let final_in = final_t.input_tokens + final_t.cache_read_tokens;
            if sum_in >= final_in {
                sum_ge_final += 1;
                max_delta = max_delta.max(sum_in - final_in);
            }
            eprintln!(
                "{}: sum_in={sum_in} final_in={final_in} sum_out={} final_out={}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                session.totals.output_tokens,
                final_t.output_tokens,
            );
        });
        eprintln!(
            "codex probe: files={checked} sum>=final={sum_ge_final} max_delta_input_side={max_delta}"
        );
        // Soft invariant: `sum(last)` is upward-bounded by reality; we never
        // want to see `sum(last) < final` (would mean we dropped a turn).
        assert_eq!(
            checked, sum_ge_final,
            "sum(last) must never underrun final total on real rollouts"
        );
    }

    fn walk_rollouts(dir: &Path, cb: &mut impl FnMut(&Path)) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk_rollouts(&p, cb);
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                cb(&p);
            }
        }
    }
}
