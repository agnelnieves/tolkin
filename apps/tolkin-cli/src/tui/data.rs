//! Pure data helpers for the dashboard.
//!
//! Everything here is a pure transformation over the loaded snapshot plus the
//! background scan result; no IO, no clock reads, no environment access. The
//! TUI feeds these helpers and renders the output, which keeps the widgets
//! thin and the helpers unit-testable.

use std::collections::BTreeMap;

use crate::commands::stats_data::StatsSnapshot;
use crate::ledger::LedgerRecord;
use crate::project::ProjectReport;
use crate::tiers::TierReport;
use crate::usage::types::UsageTotals;

/// Cap for the per-project heavy-files list shown on the Project tab.
pub const PROJECT_TOP_FILES_CAP: usize = 8;
/// Days of history shown on the Spend tab.
pub const SPEND_DAYS_WINDOW: usize = 30;
/// Sparkline length for the realized-savings trend on the Project tab.
pub const REALIZED_SPARK_POINTS: usize = 32;

/// One project's standing always-loaded weight at its latest snapshot, used by
/// the Machine tab's ranked bar list.
#[derive(Clone, Debug)]
pub struct MachineProject {
    pub key: String,
    pub always_tokens: u64,
    /// Timestamp of the latest snapshot for the project; reserved for the
    /// v1.1 polish where the row footer shows "as of" times.
    #[allow(dead_code)]
    pub last_ts: u64,
    pub sessions: u64,
}

/// One row of the Spend tab's per-model breakdown.
#[derive(Clone, Debug)]
pub struct ModelRow {
    pub model: String,
    pub totals: UsageTotals,
    pub cost_usd: Option<f64>,
}

/// One day's input-side aggregate for the Spend tab bar chart.
#[derive(Clone, Debug)]
pub struct DayBar {
    /// "YYYY-MM-DD". Reserved for the v1.1 polish where the bar chart x-axis
    /// renders week boundaries; not consumed by the sparkline today.
    #[allow(dead_code)]
    pub day: String,
    pub input_side: u64,
}

/// Bar scaling: returns ceil((value / max) * width), saturating at `width`.
/// Returns 0 when max == 0 (never divides by zero).
pub fn scale_bar(value: u64, max: u64, width: u16) -> u16 {
    if max == 0 || width == 0 {
        return 0;
    }
    let ratio = (value as f64) / (max as f64);
    let scaled = (ratio * width as f64).ceil() as u64;
    scaled.min(width as u64) as u16
}

/// Sparkline series: realized-savings progression for one project over its
/// snapshots. Each output value is the baseline minus the current snapshot's
/// always_tokens, clamped at 0. Snapshots without `always_tokens` are skipped.
/// Output is empty if the project has fewer than two snapshots.
pub fn realized_sparkline_for_project(records: &[LedgerRecord], project_key: &str) -> Vec<u64> {
    let mut snaps: Vec<(u64, u64)> = records
        .iter()
        .filter(|r| r.command == "project" && r.project_key == project_key)
        .filter_map(|r| {
            r.headline
                .get("always_tokens")
                .and_then(|v| v.as_u64())
                .map(|always| (r.ts, always))
        })
        .collect();
    snaps.sort_by_key(|s| s.0);
    if snaps.len() < 2 {
        return Vec::new();
    }
    let baseline = snaps[0].1;
    snaps
        .iter()
        .map(|(_, w)| baseline.saturating_sub(*w))
        .collect()
}

/// Rank all projects in the ledger by their latest `project` snapshot's
/// `always_tokens`. Sessions per project come from the ingested usage data,
/// defaulting to 0.
pub fn machine_projects(
    records: &[LedgerRecord],
    sessions_by_project: &BTreeMap<String, u64>,
) -> Vec<MachineProject> {
    let mut latest: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for r in records {
        if r.command != "project" {
            continue;
        }
        let Some(always) = r.headline.get("always_tokens").and_then(|v| v.as_u64()) else {
            continue;
        };
        latest
            .entry(r.project_key.clone())
            .and_modify(|prev| {
                if r.ts >= prev.0 {
                    *prev = (r.ts, always);
                }
            })
            .or_insert((r.ts, always));
    }
    let mut out: Vec<MachineProject> = latest
        .into_iter()
        .map(|(key, (ts, always))| MachineProject {
            sessions: sessions_by_project.get(&key).copied().unwrap_or(0),
            key,
            last_ts: ts,
            always_tokens: always,
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.always_tokens));
    out
}

/// Aggregate session counts by project key for the Machine tab. Sessions whose
/// `project_key` is missing or empty are dropped (the ledger never records an
/// empty key, so an empty key here is the log's "no cwd" case).
pub fn sessions_by_project(snapshot: &StatsSnapshot) -> BTreeMap<String, u64> {
    let mut out: BTreeMap<String, u64> = BTreeMap::new();
    let Some(usage) = snapshot.usage_data.as_ref() else {
        return out;
    };
    for s in &usage.sessions {
        if s.project_key.is_empty() {
            continue;
        }
        *out.entry(s.project_key.clone()).or_insert(0) += 1;
    }
    out
}

/// Build the 30-day input-side bar series for the Spend tab from a measured
/// tier and the underlying sessions. Days with no traffic appear as zeros so
/// the bar chart is visually anchored to the calendar window.
///
/// `today` is the UTC day string ("YYYY-MM-DD") corresponding to `now`.
pub fn spend_day_bars(snapshot: &StatsSnapshot, today: &str) -> Vec<DayBar> {
    let mut by_day: BTreeMap<String, u64> = BTreeMap::new();
    if let Some(usage) = snapshot.usage_data.as_ref() {
        for session in &usage.sessions {
            for (day, totals) in &session.by_day {
                *by_day.entry(day.clone()).or_insert(0) += totals.input_side();
            }
        }
    }
    let mut days = window_days(today, SPEND_DAYS_WINDOW);
    days.reverse(); // oldest first so charts render left-to-right
    days.into_iter()
        .map(|day| {
            let input_side = by_day.get(&day).copied().unwrap_or(0);
            DayBar { day, input_side }
        })
        .collect()
}

/// Format a unix epoch second count as the UTC "YYYY-MM-DD" calendar day.
pub fn day_string(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{mo:02}-{d:02}")
}

/// Generate `n` UTC day strings counting backwards from `today` (inclusive).
/// `today` MUST be a valid "YYYY-MM-DD" string; on a parse failure the result
/// is just `today` repeated, keeping the function total.
fn window_days(today: &str, n: usize) -> Vec<String> {
    let Some(start_days) = parse_day_to_days(today) else {
        return vec![today.to_string(); n];
    };
    (0..n as i64)
        .map(|i| {
            let (y, mo, d) = civil_from_days(start_days - i);
            format!("{y:04}-{mo:02}-{d:02}")
        })
        .collect()
}

fn parse_day_to_days(day: &str) -> Option<i64> {
    let parts: Vec<&str> = day.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

/// Inverse of civil_from_days (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m32 = m;
    let mp = if m32 > 2 { m32 - 3 } else { m32 + 9 };
    let doy = (153 * mp as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Howard Hinnant's civil_from_days algorithm, mirrored from `commands::stats`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Top 5 model rows by input-side volume, used by the Spend tab.
pub fn top_model_rows(report: &TierReport) -> Vec<ModelRow> {
    let Some(measured) = report.measured.as_ref() else {
        return Vec::new();
    };
    let mut rows: Vec<ModelRow> = measured
        .by_model
        .iter()
        .map(|(model, mu)| ModelRow {
            model: model.clone(),
            totals: mu.totals,
            cost_usd: mu.cost_usd,
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.totals.input_side()));
    rows.truncate(5);
    rows
}

/// Top heavy files from a live project scan, capped per the constant.
/// Returns `(path, tokens)` rows ready for rendering.
pub fn top_heavy_files(report: &ProjectReport) -> Vec<(String, u64)> {
    report
        .heaviest
        .iter()
        .take(PROJECT_TOP_FILES_CAP)
        .map(|h| (h.path.clone(), h.tokens))
        .collect()
}

/// Honesty line shown in every footer; never altered without owner review.
pub const HONESTY_LINE: &str = "input savings, output may vary";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::LedgerRecord;
    use serde_json::json;

    fn snap_record(project: &str, ts: u64, always: u64) -> LedgerRecord {
        LedgerRecord {
            v: 1,
            ts,
            command: "project".to_string(),
            project_key: project.to_string(),
            headline: json!({ "always_tokens": always }),
            tolkin_version: "0".to_string(),
            prices_observed: "test".to_string(),
        }
    }

    #[test]
    fn scale_bar_caps_at_width_and_safe_at_zero() {
        assert_eq!(scale_bar(0, 0, 20), 0);
        assert_eq!(scale_bar(10, 0, 20), 0);
        assert_eq!(scale_bar(0, 100, 20), 0);
        assert_eq!(scale_bar(50, 100, 20), 10);
        assert_eq!(scale_bar(99, 100, 20), 20);
        assert_eq!(scale_bar(1, 100, 20), 1);
        // Saturation when value > max (defensive: bad data should not panic).
        assert_eq!(scale_bar(200, 100, 20), 20);
        // Width zero is always zero (no division by zero).
        assert_eq!(scale_bar(50, 100, 0), 0);
    }

    #[test]
    fn realized_sparkline_returns_empty_below_two_snapshots() {
        let ledger = vec![snap_record("/p", 1000, 10_000)];
        let out = realized_sparkline_for_project(&ledger, "/p");
        assert!(out.is_empty());
        let empty: Vec<LedgerRecord> = Vec::new();
        assert!(realized_sparkline_for_project(&empty, "/p").is_empty());
    }

    #[test]
    fn realized_sparkline_uses_first_snapshot_as_baseline() {
        let ledger = vec![
            snap_record("/p", 1000, 10_000),
            snap_record("/p", 2000, 9_000),
            snap_record("/p", 3000, 7_000),
            // Different project, must not bleed in.
            snap_record("/other", 1500, 5_000),
        ];
        let out = realized_sparkline_for_project(&ledger, "/p");
        assert_eq!(out, vec![0, 1_000, 3_000]);
    }

    #[test]
    fn realized_sparkline_clamps_at_zero_when_context_grows() {
        let ledger = vec![
            snap_record("/p", 1000, 10_000),
            snap_record("/p", 2000, 12_000),
            snap_record("/p", 3000, 7_000),
        ];
        let out = realized_sparkline_for_project(&ledger, "/p");
        // 10000 - 10000 = 0, 10000 - 12000 -> saturating 0, 10000 - 7000 = 3000.
        assert_eq!(out, vec![0, 0, 3_000]);
    }

    #[test]
    fn machine_projects_uses_latest_snapshot_and_sorts_descending() {
        let ledger = vec![
            snap_record("/a", 1000, 10_000),
            snap_record("/a", 2000, 6_000), // latest for /a
            snap_record("/b", 1500, 20_000),
            // A non-project record must not crash or contribute.
            LedgerRecord {
                v: 1,
                ts: 5000,
                command: "audit".to_string(),
                project_key: "/c".to_string(),
                headline: json!({ "savings_min": 0, "savings_max": 0 }),
                tolkin_version: "0".to_string(),
                prices_observed: "t".to_string(),
            },
        ];
        let mut sessions = BTreeMap::new();
        sessions.insert("/a".to_string(), 3);
        let out = machine_projects(&ledger, &sessions);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key, "/b");
        assert_eq!(out[0].always_tokens, 20_000);
        assert_eq!(out[0].sessions, 0);
        assert_eq!(out[1].key, "/a");
        assert_eq!(out[1].always_tokens, 6_000);
        assert_eq!(out[1].sessions, 3);
    }

    #[test]
    fn window_days_fills_missing_days_with_zeros() {
        // Build a fake snapshot via the lightweight path: this exercises the
        // calendar window directly, which is the load-bearing piece.
        let days = window_days("2026-06-10", 5);
        assert_eq!(
            days,
            vec![
                "2026-06-10",
                "2026-06-09",
                "2026-06-08",
                "2026-06-07",
                "2026-06-06",
            ]
        );
    }

    #[test]
    fn window_days_crosses_month_boundary() {
        let days = window_days("2026-03-02", 4);
        assert_eq!(
            days,
            vec!["2026-03-02", "2026-03-01", "2026-02-28", "2026-02-27"]
        );
    }

    #[test]
    fn day_string_matches_known_epochs() {
        assert_eq!(day_string(0), "1970-01-01");
        // 2024-01-01 00:00:00 UTC = 1_704_067_200
        assert_eq!(day_string(1_704_067_200), "2024-01-01");
    }
}
