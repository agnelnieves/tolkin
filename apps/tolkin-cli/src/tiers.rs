//! Three-tier savings accounting (identified, realized, measured).
//!
//! Pure module: no IO, no environment access, no clock reads. Everything,
//! including `now`, arrives through [`TierInputs`]. Every savings number
//! Tolkin surfaces belongs to exactly one tier and always carries its label:
//!
//! - Tier 1, identified, "advisory estimate": what `project`, `scan`/`mcp`,
//!   and `audit` currently flag as reclaimable, read from the latest ledger
//!   record per component in scope.
//! - Tier 2, realized, "measured structure, estimated frequency": input
//!   tokens never sent because the always-loaded context weight dropped
//!   between ledger snapshots. The formula below is the audit trail for
//!   every realized headline; anyone can recompute it from their ledger.
//! - Tier 3, measured, "ground truth": actual spend aggregated from local
//!   agent session logs, cache-aware.
//!
//! # The realized-savings formula
//!
//! For one project, order its `project` ledger snapshots by timestamp. A
//! snapshot is a `project` record whose headline carries `always_tokens`
//! (the always-loaded context weight measured at that moment); `project`
//! records without it are not snapshots and are skipped.
//!
//! - `baseline` is the `always_tokens` of the first snapshot.
//! - `w_active_at(t)` is the `always_tokens` of the latest snapshot taken at
//!   or before `t`: the standing weight a session starting at `t` actually
//!   paid.
//!
//! Measured mode (usage logs ingested):
//!
//! ```text
//! realized_tokens = sum over sessions with first_ts >= first_snapshot.ts
//!                   of (baseline - w_active_at(session.first_ts))
//! ```
//!
//! Each counted session paid the standing weight current at its start
//! instead of the baseline weight; the difference is input tokens that were
//! never sent. Sessions started before the first snapshot have no
//! counterfactual and are excluded entirely. `sessions_count` counts the
//! sessions included in the sum: every session starting at or after the
//! first snapshot, including sessions that contribute zero because they ran
//! while the baseline weight was still standing.
//!
//! Assumed mode (no usage logs): the continuous analog over snapshot
//! intervals, where interval `i` opens at snapshot `i` and closes at the
//! next snapshot (the last interval closes at `now`), and `w_i` is the
//! `always_tokens` of the snapshot opening the interval:
//!
//! ```text
//! realized_tokens = sum over intervals of
//!     (baseline - w_i) * assumed_rate_per_day * (interval_seconds / 86400)
//! ```
//!
//! The first interval is opened by the baseline snapshot itself and
//! contributes zero by construction. The final sum is rounded to a whole
//! token count; `sessions_count` is the rounded synthetic count
//! `assumed_rate_per_day * (now - first_snapshot.ts) / 86400`.
//!
//! The result is signed. If the standing context grew, realized savings go
//! negative and are reported as such, never clamped and never hidden.
//! Dollars carry the same sign:
//!
//! ```text
//! usd = realized_tokens / 1_000_000 * realized_usd_per_mtok
//! ```
//!
//! In global scope the formula runs per project and the contributions are
//! summed; only projects with two or more snapshots contribute.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::ledger::LedgerRecord;
use crate::usage::types::{SessionUsage, UsageTotals};

/// Tier 1 label, serialized verbatim with every identified number.
pub const LABEL_IDENTIFIED: &str = "advisory estimate";
/// Tier 2 label, serialized verbatim with every realized number.
pub const LABEL_REALIZED: &str = "measured structure, estimated frequency";
/// Tier 3 label, serialized verbatim with every measured number.
pub const LABEL_MEASURED: &str = "ground truth";

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Inputs for [`compute`]. The module never reads the environment or the
/// clock; the caller supplies everything, which keeps every report
/// reproducible from its inputs.
pub struct TierInputs<'a> {
    /// `None` = global scope (all projects in the ledger).
    pub scope_project: Option<&'a str>,
    pub ledger: &'a [LedgerRecord],
    /// `None` = usage-log ingestion is off; realized falls back to the
    /// assumed session rate.
    pub sessions: Option<&'a [SessionUsage]>,
    /// Sessions-per-day fallback for assumed-mode realized savings.
    pub assumed_rate_per_day: f64,
    /// Dollars per million input tokens for the realized conversion.
    pub realized_usd_per_mtok: f64,
    /// Injected pricing for measured-mode costs, called with the RAW model
    /// id. `None` marks the model as unpriced.
    pub cost_fn: &'a dyn Fn(&str, &UsageTotals) -> Option<f64>,
    /// Epoch seconds, closing the last assumed-mode interval.
    pub now: u64,
}

#[derive(Debug, Serialize)]
pub struct TierReport {
    pub identified: Option<Identified>,
    pub realized: Option<Realized>,
    pub measured: Option<Measured>,
    pub notes: Vec<String>,
}

/// Tier 1: savings flagged as reclaimable right now ("advisory estimate").
///
/// Each component comes from the latest matching ledger record in scope. The
/// MCP component prefers the latest `scan` record over the latest `mcp`
/// record, regardless of which is newer, because `scan` aggregates every
/// discovered config; summing scan and mcp would double count the same
/// servers. The flip side: a stale scan keeps shadowing fresher mcp records
/// until the next scan, so re-run `tolkin scan` to refresh this component
/// (the `mcp_as_of` timestamp makes the staleness visible). In global scope
/// each component sums the per-project latest records and `as_of` is the
/// timestamp of the newest record contributing to that component's sum.
#[derive(Debug, Serialize)]
pub struct Identified {
    pub label: &'static str,
    pub project_reclaimable_min: Option<u64>,
    pub project_reclaimable_max: Option<u64>,
    pub project_as_of: Option<u64>,
    pub mcp_swap_tokens: Option<u64>,
    pub mcp_slim_tokens: Option<u64>,
    pub mcp_as_of: Option<u64>,
    pub audit_savings_min: Option<u64>,
    pub audit_savings_max: Option<u64>,
    pub audit_as_of: Option<u64>,
    /// Count of projects contributing at least one component.
    pub projects: u64,
}

/// Tier 2: realized savings ("measured structure, estimated frequency"),
/// computed by the formula in the module docs.
///
/// `tokens` and `usd` are signed: context growth reports as negative.
/// `sessions_count` counts the sessions included in the sum, meaning every
/// session whose `first_ts` is at or after the first snapshot, including
/// zero-contribution sessions that ran while the baseline weight was still
/// standing. In assumed mode it is the rounded synthetic count
/// `assumed_rate_per_day * (now - since) / 86400`.
#[derive(Debug, Serialize)]
pub struct Realized {
    pub label: &'static str,
    pub tokens: i64,
    pub usd: f64,
    /// "measured" (real session logs) or "assumed" (session-rate fallback).
    pub sessions_basis: &'static str,
    pub sessions_count: u64,
    /// Timestamp of the first snapshot (earliest across projects in global
    /// scope).
    pub since: u64,
    /// Sum of contributing projects' baseline `always_tokens`.
    pub baseline_always_tokens: u64,
    /// Sum of contributing projects' latest-snapshot `always_tokens`.
    pub current_always_tokens: u64,
    /// Count of projects contributing (those with two or more snapshots).
    pub projects: u64,
}

/// Tier 3: actual spend from ingested session logs ("ground truth").
///
/// `cache_hit_rate` is the share of input-side tokens served from cache:
///
/// ```text
/// cache_hit_rate = cache_read_tokens
///     / (input_tokens + cache_read_tokens
///        + cache_write_5m_tokens + cache_write_1h_tokens)
/// ```
///
/// and is 0.0 when the denominator is zero. `cost_usd_total` sums priced
/// models only; models the pricing function cannot price are listed in
/// `unpriced_models` with their token volume so the gap stays visible.
#[derive(Debug, Serialize)]
pub struct Measured {
    pub label: &'static str,
    pub sessions: u64,
    pub first_ts: Option<u64>,
    pub last_ts: Option<u64>,
    pub totals: UsageTotals,
    /// Keyed by RAW model id as it appears in the logs.
    pub by_model: BTreeMap<String, ModelUsage>,
    pub cost_usd_total: f64,
    /// Sorted by model id (BTreeMap iteration order).
    pub unpriced_models: Vec<UnpricedModel>,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct ModelUsage {
    pub totals: UsageTotals,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct UnpricedModel {
    pub model: String,
    /// Total token volume for the model: input side plus output.
    pub tokens: u64,
}

/// Compute the three-tier report from a ledger, optional session logs, and
/// the caller-supplied scope, rates, and clock. See the module docs for the
/// realized-savings formula.
pub fn compute(inputs: &TierInputs) -> TierReport {
    let by_project = scoped_by_project(inputs);
    let identified = compute_identified(&by_project);
    let realized = compute_realized(inputs, &by_project);
    let measured = compute_measured(inputs);
    let notes = build_notes(inputs, realized.as_ref(), measured.as_ref());
    TierReport {
        identified,
        realized,
        measured,
        notes,
    }
}

/// Group scoped ledger records by project key, preserving input order within
/// each project (the tie-break for equal timestamps).
fn scoped_by_project<'a>(inputs: &TierInputs<'a>) -> BTreeMap<&'a str, Vec<&'a LedgerRecord>> {
    let mut map: BTreeMap<&str, Vec<&LedgerRecord>> = BTreeMap::new();
    for record in inputs.ledger {
        if let Some(scope) = inputs.scope_project {
            if record.project_key != scope {
                continue;
            }
        }
        map.entry(record.project_key.as_str())
            .or_default()
            .push(record);
    }
    map
}

/// Latest record for `command` by (ts, then input order): on equal
/// timestamps the record appearing later in the ledger wins.
fn latest_of<'a>(records: &[&'a LedgerRecord], command: &str) -> Option<&'a LedgerRecord> {
    let mut best: Option<&LedgerRecord> = None;
    for record in records {
        if record.command != command {
            continue;
        }
        if best.is_none_or(|b| record.ts >= b.ts) {
            best = Some(record);
        }
    }
    best
}

/// Defensive headline read: missing, non-numeric, or non-object headlines
/// all collapse to 0.
fn field_u64(record: &LedgerRecord, key: &str) -> u64 {
    record
        .headline
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// One identified component: (first value, second value, as_of timestamp).
type Component = Option<(u64, u64, u64)>;

fn add_component(acc: &mut Component, part: Component) {
    let Some((a, b, ts)) = part else { return };
    match acc {
        Some((acc_a, acc_b, acc_ts)) => {
            // Ledger JSONL is user-editable; absurd values saturate instead
            // of panicking in debug builds.
            *acc_a = acc_a.saturating_add(a);
            *acc_b = acc_b.saturating_add(b);
            *acc_ts = (*acc_ts).max(ts);
        }
        None => *acc = Some((a, b, ts)),
    }
}

/// Saturating u64 to i64 conversion: hand-edited ledgers can carry values
/// past i64::MAX, which `as i64` would wrap negative.
fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn split_component(part: Component) -> (Option<u64>, Option<u64>, Option<u64>) {
    match part {
        Some((a, b, ts)) => (Some(a), Some(b), Some(ts)),
        None => (None, None, None),
    }
}

fn compute_identified(by_project: &BTreeMap<&str, Vec<&LedgerRecord>>) -> Option<Identified> {
    let mut project_acc: Component = None;
    let mut mcp_acc: Component = None;
    let mut audit_acc: Component = None;
    let mut projects: u64 = 0;
    for records in by_project.values() {
        let project = latest_of(records, "project").map(|r| {
            (
                field_u64(r, "reclaimable_min"),
                field_u64(r, "reclaimable_max"),
                r.ts,
            )
        });
        // Scan supersedes mcp categorically (even an older scan): scan
        // aggregates every discovered config, so summing both would double
        // count the same servers.
        let mcp = latest_of(records, "scan")
            .or_else(|| latest_of(records, "mcp"))
            .map(|r| {
                (
                    field_u64(r, "swap_savings_tokens"),
                    field_u64(r, "slim_savings_tokens"),
                    r.ts,
                )
            });
        let audit = latest_of(records, "audit").map(|r| {
            (
                field_u64(r, "savings_min"),
                field_u64(r, "savings_max"),
                r.ts,
            )
        });
        if project.is_some() || mcp.is_some() || audit.is_some() {
            projects += 1;
        }
        add_component(&mut project_acc, project);
        add_component(&mut mcp_acc, mcp);
        add_component(&mut audit_acc, audit);
    }
    if projects == 0 {
        return None;
    }
    let (project_reclaimable_min, project_reclaimable_max, project_as_of) =
        split_component(project_acc);
    let (mcp_swap_tokens, mcp_slim_tokens, mcp_as_of) = split_component(mcp_acc);
    let (audit_savings_min, audit_savings_max, audit_as_of) = split_component(audit_acc);
    Some(Identified {
        label: LABEL_IDENTIFIED,
        project_reclaimable_min,
        project_reclaimable_max,
        project_as_of,
        mcp_swap_tokens,
        mcp_slim_tokens,
        mcp_as_of,
        audit_savings_min,
        audit_savings_max,
        audit_as_of,
        projects,
    })
}

/// Snapshots for one project: `project` records carrying `always_tokens`,
/// as (ts, always_tokens), stable-sorted by ts so equal timestamps keep
/// input order (the later record is the later snapshot).
fn snapshots(records: &[&LedgerRecord]) -> Vec<(u64, u64)> {
    let mut snaps: Vec<(u64, u64)> = records
        .iter()
        .filter(|r| r.command == "project")
        .filter_map(|r| {
            r.headline
                .get("always_tokens")
                .and_then(Value::as_u64)
                .map(|always| (r.ts, always))
        })
        .collect();
    snaps.sort_by_key(|s| s.0);
    snaps
}

/// `w_active_at(t)`: always_tokens of the latest snapshot with ts <= t.
/// Callers guarantee `snaps` is non-empty and t >= snaps[0].0.
fn active_weight(snaps: &[(u64, u64)], t: u64) -> u64 {
    let mut weight = snaps[0].1;
    for &(ts, always) in snaps {
        if ts <= t {
            weight = always;
        } else {
            break;
        }
    }
    weight
}

fn compute_realized(
    inputs: &TierInputs,
    by_project: &BTreeMap<&str, Vec<&LedgerRecord>>,
) -> Option<Realized> {
    let mut tokens_measured: i64 = 0;
    let mut tokens_assumed: f64 = 0.0;
    let mut count_measured: u64 = 0;
    let mut count_assumed: f64 = 0.0;
    let mut since: Option<u64> = None;
    let mut baseline_sum: u64 = 0;
    let mut current_sum: u64 = 0;
    let mut projects: u64 = 0;

    for (key, records) in by_project {
        let snaps = snapshots(records);
        if snaps.len() < 2 {
            continue;
        }
        projects += 1;
        let first_ts = snaps[0].0;
        let baseline = snaps[0].1;
        baseline_sum += baseline;
        current_sum += snaps[snaps.len() - 1].1;
        since = Some(since.map_or(first_ts, |s| s.min(first_ts)));
        match inputs.sessions {
            Some(sessions) => {
                for session in sessions
                    .iter()
                    .filter(|s| s.project_key == *key && s.first_ts >= first_ts)
                {
                    let weight = active_weight(&snaps, session.first_ts);
                    tokens_measured += to_i64(baseline) - to_i64(weight);
                    count_measured += 1;
                }
            }
            None => {
                for (i, &(ts, weight)) in snaps.iter().enumerate() {
                    let raw_end = if i + 1 < snaps.len() {
                        snaps[i + 1].0
                    } else {
                        inputs.now
                    };
                    // Both bounds clamp to now: future-dated snapshots (clock
                    // skew, copied ledgers) must not accrue assumed sessions
                    // that have not happened yet. A clock that ran backwards
                    // yields zero-length intervals, contributing zero.
                    let end = raw_end.min(inputs.now);
                    let start = ts.min(end);
                    let days = (end - start) as f64 / SECONDS_PER_DAY;
                    tokens_assumed +=
                        (baseline as f64 - weight as f64) * inputs.assumed_rate_per_day * days;
                }
                count_assumed += inputs.assumed_rate_per_day
                    * (inputs.now.saturating_sub(first_ts) as f64)
                    / SECONDS_PER_DAY;
            }
        }
    }
    if projects == 0 {
        return None;
    }
    let (tokens, sessions_basis, sessions_count) = match inputs.sessions {
        Some(_) => (tokens_measured, "measured", count_measured),
        None => (
            tokens_assumed.round() as i64,
            "assumed",
            count_assumed.round() as u64,
        ),
    };
    let usd = tokens as f64 / 1_000_000.0 * inputs.realized_usd_per_mtok;
    Some(Realized {
        label: LABEL_REALIZED,
        tokens,
        usd,
        sessions_basis,
        sessions_count,
        since: since.unwrap_or(0),
        baseline_always_tokens: baseline_sum,
        current_always_tokens: current_sum,
        projects,
    })
}

fn compute_measured(inputs: &TierInputs) -> Option<Measured> {
    let sessions = inputs.sessions?;
    let mut totals = UsageTotals::default();
    let mut by_model_totals: BTreeMap<String, UsageTotals> = BTreeMap::new();
    let mut count: u64 = 0;
    let mut first_ts: Option<u64> = None;
    let mut last_ts: Option<u64> = None;
    for session in sessions {
        if let Some(scope) = inputs.scope_project {
            if session.project_key != scope {
                continue;
            }
        }
        count += 1;
        totals.add(&session.totals);
        first_ts = Some(first_ts.map_or(session.first_ts, |t| t.min(session.first_ts)));
        last_ts = Some(last_ts.map_or(session.last_ts, |t| t.max(session.last_ts)));
        for (model, model_totals) in &session.by_model {
            by_model_totals
                .entry(model.clone())
                .or_default()
                .add(model_totals);
        }
    }
    let mut cost_usd_total = 0.0;
    let mut unpriced_models = Vec::new();
    let mut by_model = BTreeMap::new();
    for (model, model_totals) in by_model_totals {
        let cost_usd = (inputs.cost_fn)(&model, &model_totals);
        match cost_usd {
            Some(cost) => cost_usd_total += cost,
            None => unpriced_models.push(UnpricedModel {
                model: model.clone(),
                tokens: model_totals.input_side() + model_totals.output_tokens,
            }),
        }
        by_model.insert(
            model,
            ModelUsage {
                totals: model_totals,
                cost_usd,
            },
        );
    }
    let denominator = totals.input_side();
    let cache_hit_rate = if denominator == 0 {
        0.0
    } else {
        totals.cache_read_tokens as f64 / denominator as f64
    };
    Some(Measured {
        label: LABEL_MEASURED,
        sessions: count,
        first_ts,
        last_ts,
        totals,
        by_model,
        cost_usd_total,
        unpriced_models,
        cache_hit_rate,
    })
}

fn build_notes(
    inputs: &TierInputs,
    realized: Option<&Realized>,
    measured: Option<&Measured>,
) -> Vec<String> {
    let mut notes = vec!["All savings are input-token bounded; output may vary.".to_string()];
    if realized.is_some_and(|r| r.sessions_basis == "assumed") {
        notes.push(format!(
            "Realized savings use an assumed session rate of {}/day; ingest usage logs for measured counts.",
            inputs.assumed_rate_per_day
        ));
    }
    if let Some(measured) = measured {
        if !measured.unpriced_models.is_empty() {
            let names: Vec<&str> = measured
                .unpriced_models
                .iter()
                .map(|u| u.model.as_str())
                .collect();
            notes.push(format!(
                "Unpriced models excluded from cost_usd_total: {}.",
                names.join(", ")
            ));
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::UsageSource;
    use serde_json::json;

    fn rec(command: &str, project: &str, ts: u64, headline: Value) -> LedgerRecord {
        LedgerRecord {
            v: 1,
            ts,
            command: command.to_string(),
            project_key: project.to_string(),
            headline,
            tolkin_version: "0.0.0-test".to_string(),
            prices_observed: "test".to_string(),
        }
    }

    fn snap(project: &str, ts: u64, always: u64) -> LedgerRecord {
        rec("project", project, ts, json!({ "always_tokens": always }))
    }

    fn session(project: &str, first_ts: u64, source: UsageSource) -> SessionUsage {
        SessionUsage {
            source,
            session_id: format!("s-{first_ts}"),
            project_key: project.to_string(),
            first_ts,
            last_ts: first_ts + 60,
            totals: UsageTotals::default(),
            by_model: BTreeMap::new(),
            by_day: BTreeMap::new(),
            requests: Vec::new(),
        }
    }

    fn session_with_model(
        source: UsageSource,
        project: &str,
        first_ts: u64,
        last_ts: u64,
        model: &str,
        totals: UsageTotals,
    ) -> SessionUsage {
        let mut by_model = BTreeMap::new();
        by_model.insert(model.to_string(), totals);
        SessionUsage {
            source,
            session_id: format!("s-{first_ts}"),
            project_key: project.to_string(),
            first_ts,
            last_ts,
            totals,
            by_model,
            by_day: BTreeMap::new(),
            requests: Vec::new(),
        }
    }

    fn usage(input: u64, output: u64, read: u64, w5: u64, w1: u64) -> UsageTotals {
        UsageTotals {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_5m_tokens: w5,
            cache_write_1h_tokens: w1,
        }
    }

    fn no_cost(_model: &str, _totals: &UsageTotals) -> Option<f64> {
        None
    }

    fn inputs<'a>(
        ledger: &'a [LedgerRecord],
        sessions: Option<&'a [SessionUsage]>,
    ) -> TierInputs<'a> {
        TierInputs {
            scope_project: None,
            ledger,
            sessions,
            assumed_rate_per_day: 10.0,
            realized_usd_per_mtok: 3.0,
            cost_fn: &no_cost,
            now: 10_000_000,
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // Snapshots: w=10000 at t=1000, w=8000 at t=2000. baseline = 10000.
    // Session 1500: w_active 10000 -> 10000 - 10000 = 0 (included, zero).
    // Session 2500: w_active 8000  -> 10000 - 8000  = 2000.
    // Session 3000: w_active 8000  -> 2000.
    // realized = 0 + 2000 + 2000 = 4000. sessions_count = 3 (the zero
    // contributor is included: first_ts >= first snapshot). Sources mixed on
    // purpose: ClaudeCode and Codex sessions count equally.
    // usd = 4000 / 1_000_000 * 3.0 = 0.012.
    #[test]
    fn single_drop_counts_zero_contribution_sessions() {
        let ledger = vec![snap("/p", 1000, 10_000), snap("/p", 2000, 8_000)];
        let sessions = vec![
            session("/p", 1500, UsageSource::ClaudeCode),
            session("/p", 2500, UsageSource::Codex),
            session("/p", 3000, UsageSource::ClaudeCode),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 4000);
        assert!(approx(realized.usd, 0.012));
        assert_eq!(realized.sessions_basis, "measured");
        assert_eq!(realized.sessions_count, 3);
        assert_eq!(realized.since, 1000);
        assert_eq!(realized.baseline_always_tokens, 10_000);
        assert_eq!(realized.current_always_tokens, 8_000);
        assert_eq!(realized.projects, 1);
        assert_eq!(realized.label, "measured structure, estimated frequency");
        // Measured basis: no assumed-rate note; no unpriced models either.
        assert_eq!(
            report.notes,
            ["All savings are input-token bounded; output may vary."]
        );
    }

    // Snapshots: 10000 at t=1000, 8000 at t=2000, 5000 at t=3000.
    // baseline = 10000.
    // Window [1000, 2000): w 10000 -> delta 0 per session.
    // Window [2000, 3000): w 8000  -> delta 2000 per session.
    // Window [3000, ...):  w 5000  -> delta 5000 per session.
    // Sessions: 1100 (0), 2100 (+2000), 2900 (+2000), 3100 (+5000),
    // 4000 (+5000). realized = 0 + 2000 + 2000 + 5000 + 5000 = 14000.
    // sessions_count = 5.
    #[test]
    fn multi_drop_sums_per_window() {
        let ledger = vec![
            snap("/p", 1000, 10_000),
            snap("/p", 2000, 8_000),
            snap("/p", 3000, 5_000),
        ];
        let sessions = vec![
            session("/p", 1100, UsageSource::ClaudeCode),
            session("/p", 2100, UsageSource::ClaudeCode),
            session("/p", 2900, UsageSource::Codex),
            session("/p", 3100, UsageSource::ClaudeCode),
            session("/p", 4000, UsageSource::Codex),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 14_000);
        assert_eq!(realized.sessions_count, 5);
        assert_eq!(realized.baseline_always_tokens, 10_000);
        assert_eq!(realized.current_always_tokens, 5_000);
    }

    // Snapshots: 10000 at t=1000, 12000 at t=2000 (growth), 7000 at t=3000.
    // baseline = 10000.
    // Sessions 2100 and 2200: w 12000 -> 10000 - 12000 = -2000 each.
    // Session 3100: w 7000 -> 10000 - 7000 = +3000.
    // realized = -2000 - 2000 + 3000 = -1000 (signed, never clamped).
    // usd = -1000 / 1_000_000 * 3.0 = -0.003.
    #[test]
    fn growth_then_drop_reports_negative() {
        let ledger = vec![
            snap("/p", 1000, 10_000),
            snap("/p", 2000, 12_000),
            snap("/p", 3000, 7_000),
        ];
        let sessions = vec![
            session("/p", 2100, UsageSource::ClaudeCode),
            session("/p", 2200, UsageSource::ClaudeCode),
            session("/p", 3100, UsageSource::ClaudeCode),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, -1000);
        assert!(realized.usd < 0.0);
        assert!(approx(realized.usd, -0.003));
        assert_eq!(realized.sessions_count, 3);
        assert_eq!(realized.current_always_tokens, 7_000);
    }

    // Snapshots: 10000 at t=1000, 8000 at t=2000. Session 500 starts before
    // the first snapshot: no counterfactual, excluded from sum AND count.
    // Session 2500: +2000. realized = 2000, sessions_count = 1.
    #[test]
    fn sessions_before_baseline_are_excluded() {
        let ledger = vec![snap("/p", 1000, 10_000), snap("/p", 2000, 8_000)];
        let sessions = vec![
            session("/p", 500, UsageSource::ClaudeCode),
            session("/p", 2500, UsageSource::ClaudeCode),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 2000);
        assert_eq!(realized.sessions_count, 1);
    }

    // Assumed mode. Snapshots: 10000 at t=0, 8000 at t=86400. now = 259200
    // (day 3), rate = 10/day.
    // Interval [0, 86400):      (10000 - 10000) * 10 * 1 day = 0 (baseline
    //                           interval, zero by construction).
    // Interval [86400, 259200): (10000 - 8000) * 10 * 2 days = 40000.
    // realized = 40000.
    // synthetic count = 10 * (259200 - 0) / 86400 = 30.
    // usd = 40000 / 1_000_000 * 3.0 = 0.12.
    #[test]
    fn assumed_mode_integrates_intervals_at_rate() {
        let ledger = vec![snap("/p", 0, 10_000), snap("/p", 86_400, 8_000)];
        let mut tier_inputs = inputs(&ledger, None);
        tier_inputs.now = 259_200;
        let report = compute(&tier_inputs);
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 40_000);
        assert!(approx(realized.usd, 0.12));
        assert_eq!(realized.sessions_basis, "assumed");
        assert_eq!(realized.sessions_count, 30);
        assert_eq!(realized.since, 0);
        assert!(report.notes.iter().any(|n| n
            == "Realized savings use an assumed session rate of 10/day; ingest usage logs for measured counts."));
    }

    // Assumed-mode clamp: snapshots at 1000 (10000), 2000 (8000), 88400
    // (5000), but now = 1500 (clock skew or a copied ledger carrying
    // future-dated records). No interval may accrue time past now:
    // Interval [1000, 2000) clamps to [1000, 1500): baseline delta 0.
    // Interval [2000, 88400) clamps to [1500, 1500): zero length.
    // Interval [88400, now)  clamps to [1500, 1500): zero length.
    // realized = 0. synthetic count = 10 * (1500 - 1000) / 86400 ~ 0.06,
    // rounds to 0. Before the clamp this case fabricated 20000 tokens of
    // savings from sessions that never happened.
    #[test]
    fn assumed_mode_never_accrues_past_now() {
        let ledger = vec![
            snap("/p", 1_000, 10_000),
            snap("/p", 2_000, 8_000),
            snap("/p", 88_400, 5_000),
        ];
        let mut tier_inputs = inputs(&ledger, None);
        tier_inputs.now = 1_500;
        let report = compute(&tier_inputs);
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 0);
        assert_eq!(realized.sessions_count, 0);
        assert!(approx(realized.usd, 0.0));
    }

    // Empty ledger: no tier can exist, but the honesty note still ships.
    #[test]
    fn empty_ledger_yields_no_tiers_and_base_note() {
        let ledger: Vec<LedgerRecord> = Vec::new();
        let report = compute(&inputs(&ledger, None));
        assert!(report.identified.is_none());
        assert!(report.realized.is_none());
        assert!(report.measured.is_none());
        assert_eq!(
            report.notes,
            ["All savings are input-token bounded; output may vary."]
        );
    }

    // One snapshot: identified yes (latest project record), realized no
    // (no second snapshot to diff against), measured no (ingestion off).
    #[test]
    fn one_snapshot_yields_identified_without_realized() {
        let ledger = vec![rec(
            "project",
            "/p",
            1234,
            json!({ "always_tokens": 10_000, "reclaimable_min": 1500, "reclaimable_max": 2400 }),
        )];
        let report = compute(&inputs(&ledger, None));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.label, "advisory estimate");
        assert_eq!(identified.project_reclaimable_min, Some(1500));
        assert_eq!(identified.project_reclaimable_max, Some(2400));
        assert_eq!(identified.project_as_of, Some(1234));
        assert_eq!(identified.mcp_swap_tokens, None);
        assert_eq!(identified.audit_savings_min, None);
        assert_eq!(identified.projects, 1);
        assert!(report.realized.is_none());
        assert!(report.measured.is_none());
    }

    // Ingestion off, audit-only ledger: identified present from the audit
    // record alone; realized and measured absent; only the base note.
    #[test]
    fn ingestion_off_audit_only_identified() {
        let ledger = vec![rec(
            "audit",
            "/p",
            2000,
            json!({ "savings_min": 50, "savings_max": 90 }),
        )];
        let report = compute(&inputs(&ledger, None));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.audit_savings_min, Some(50));
        assert_eq!(identified.audit_savings_max, Some(90));
        assert_eq!(identified.audit_as_of, Some(2000));
        assert_eq!(identified.project_reclaimable_min, None);
        assert!(report.realized.is_none());
        assert!(report.measured.is_none());
        assert_eq!(
            report.notes,
            ["All savings are input-token bounded; output may vary."]
        );
    }

    // Global scope, two projects.
    // /a: snapshots 10000 at t=1000 (reclaimable 50/75) and 8000 at t=2000
    //     (reclaimable 100/200). Latest project record wins: 100/200.
    //     Sessions 2500 and 3000: w 8000 -> +2000 each -> realized 4000,
    //     count 2.
    // /b: one snapshot 9000 at t=1500 (reclaimable 300/400) -> identified
    //     only; its session at 1700 must NOT enter realized. Scan at t=1600:
    //     swap 700, slim 300.
    // Identified sums: project 100+300=400 min, 200+400=600 max, as_of
    // max(2000, 1500)=2000; mcp 700/300 as_of 1600; projects=2.
    // Realized: only /a contributes -> 4000 tokens, projects=1, baseline
    // 10000, current 8000, since 1000.
    // Measured: all 3 sessions in scope.
    #[test]
    fn global_scope_sums_components_and_counts_projects() {
        let ledger = vec![
            rec(
                "project",
                "/a",
                1000,
                json!({ "always_tokens": 10_000, "reclaimable_min": 50, "reclaimable_max": 75 }),
            ),
            rec(
                "project",
                "/a",
                2000,
                json!({ "always_tokens": 8_000, "reclaimable_min": 100, "reclaimable_max": 200 }),
            ),
            rec(
                "project",
                "/b",
                1500,
                json!({ "always_tokens": 9_000, "reclaimable_min": 300, "reclaimable_max": 400 }),
            ),
            rec(
                "scan",
                "/b",
                1600,
                json!({ "swap_savings_tokens": 700, "slim_savings_tokens": 300 }),
            ),
        ];
        let sessions = vec![
            session("/a", 2500, UsageSource::ClaudeCode),
            session("/a", 3000, UsageSource::Codex),
            session("/b", 1700, UsageSource::ClaudeCode),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.project_reclaimable_min, Some(400));
        assert_eq!(identified.project_reclaimable_max, Some(600));
        assert_eq!(identified.project_as_of, Some(2000));
        assert_eq!(identified.mcp_swap_tokens, Some(700));
        assert_eq!(identified.mcp_slim_tokens, Some(300));
        assert_eq!(identified.mcp_as_of, Some(1600));
        assert_eq!(identified.audit_savings_min, None);
        assert_eq!(identified.projects, 2);
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 4000);
        assert_eq!(realized.sessions_count, 2);
        assert_eq!(realized.projects, 1);
        assert_eq!(realized.baseline_always_tokens, 10_000);
        assert_eq!(realized.current_always_tokens, 8_000);
        assert_eq!(realized.since, 1000);
        let measured = report.measured.expect("measured present");
        assert_eq!(measured.sessions, 3);
    }

    // Two sessions, two models, one unpriced.
    // claude-sonnet-4-5: input 400, output 1000, read 6000, w5 500, w1 0.
    // gpt-5.1-codex:     input 100, output 2000, read 3000, w5 0,   w1 0.
    // Combined totals: input 500, output 3000, read 9000, w5 500, w1 0.
    // input_side = 500 + 9000 + 500 + 0 = 10000.
    // cache_hit_rate = 9000 / 10000 = 0.9.
    // cost_fn prices sonnet at 1.25, returns None for codex:
    // cost_usd_total = 1.25 (priced models only).
    // Unpriced codex tokens = input_side + output
    //                       = (100 + 3000 + 0 + 0) + 2000 = 5100.
    #[test]
    fn measured_tier_prices_models_and_lists_unpriced() {
        let ledger: Vec<LedgerRecord> = Vec::new();
        let sessions = vec![
            session_with_model(
                UsageSource::ClaudeCode,
                "/p",
                100,
                200,
                "claude-sonnet-4-5",
                usage(400, 1000, 6000, 500, 0),
            ),
            session_with_model(
                UsageSource::Codex,
                "/p",
                300,
                400,
                "gpt-5.1-codex",
                usage(100, 2000, 3000, 0, 0),
            ),
        ];
        let price_sonnet = |model: &str, _totals: &UsageTotals| -> Option<f64> {
            (model == "claude-sonnet-4-5").then_some(1.25)
        };
        let mut tier_inputs = inputs(&ledger, Some(&sessions));
        tier_inputs.cost_fn = &price_sonnet;
        let report = compute(&tier_inputs);
        let measured = report.measured.expect("measured present");
        assert_eq!(measured.label, "ground truth");
        assert_eq!(measured.sessions, 2);
        assert_eq!(measured.first_ts, Some(100));
        assert_eq!(measured.last_ts, Some(400));
        assert_eq!(measured.totals.input_tokens, 500);
        assert_eq!(measured.totals.output_tokens, 3000);
        assert_eq!(measured.totals.cache_read_tokens, 9000);
        assert_eq!(measured.totals.cache_write_5m_tokens, 500);
        assert!(approx(measured.cache_hit_rate, 0.9));
        assert!(approx(measured.cost_usd_total, 1.25));
        assert_eq!(measured.by_model.len(), 2);
        assert_eq!(measured.by_model["claude-sonnet-4-5"].cost_usd, Some(1.25));
        assert_eq!(measured.by_model["gpt-5.1-codex"].cost_usd, None);
        assert_eq!(measured.unpriced_models.len(), 1);
        assert_eq!(measured.unpriced_models[0].model, "gpt-5.1-codex");
        assert_eq!(measured.unpriced_models[0].tokens, 5100);
        // Empty ledger: identified and realized stay absent even with
        // sessions provided.
        assert!(report.identified.is_none());
        assert!(report.realized.is_none());
        assert!(report
            .notes
            .iter()
            .any(|n| n == "Unpriced models excluded from cost_usd_total: gpt-5.1-codex."));
    }

    // All-zero totals: input_side = 0, so cache_hit_rate is defined as 0.0
    // rather than dividing by zero.
    #[test]
    fn cache_hit_rate_zero_denominator_is_zero() {
        let ledger: Vec<LedgerRecord> = Vec::new();
        let sessions = vec![session("/p", 100, UsageSource::ClaudeCode)];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let measured = report.measured.expect("measured present");
        assert_eq!(measured.sessions, 1);
        assert!(approx(measured.cache_hit_rate, 0.0));
        assert!(approx(measured.cost_usd_total, 0.0));
        assert!(measured.unpriced_models.is_empty());
    }

    // The scan record (t=1000) is OLDER than the mcp record (t=2000) but
    // still wins: scan aggregates every config, so mcp would double count.
    // as_of follows the chosen record: 1000.
    #[test]
    fn identified_scan_supersedes_newer_mcp() {
        let ledger = vec![
            rec(
                "mcp",
                "/p",
                2000,
                json!({ "swap_savings_tokens": 100, "slim_savings_tokens": 50 }),
            ),
            rec(
                "scan",
                "/p",
                1000,
                json!({ "swap_savings_tokens": 700, "slim_savings_tokens": 300 }),
            ),
        ];
        let report = compute(&inputs(&ledger, None));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.mcp_swap_tokens, Some(700));
        assert_eq!(identified.mcp_slim_tokens, Some(300));
        assert_eq!(identified.mcp_as_of, Some(1000));
    }

    // No scan record: the latest mcp record supplies the component.
    #[test]
    fn identified_uses_mcp_when_no_scan() {
        let ledger = vec![rec(
            "mcp",
            "/p",
            2000,
            json!({ "swap_savings_tokens": 100, "slim_savings_tokens": 50 }),
        )];
        let report = compute(&inputs(&ledger, None));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.mcp_swap_tokens, Some(100));
        assert_eq!(identified.mcp_slim_tokens, Some(50));
        assert_eq!(identified.mcp_as_of, Some(2000));
    }

    // Two audits: the later one (t=2000, 30/60) supersedes t=1000 (10/20).
    #[test]
    fn identified_audit_latest_wins() {
        let ledger = vec![
            rec(
                "audit",
                "/p",
                1000,
                json!({ "savings_min": 10, "savings_max": 20 }),
            ),
            rec(
                "audit",
                "/p",
                2000,
                json!({ "savings_min": 30, "savings_max": 60 }),
            ),
        ];
        let report = compute(&inputs(&ledger, None));
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.audit_savings_min, Some(30));
        assert_eq!(identified.audit_savings_max, Some(60));
        assert_eq!(identified.audit_as_of, Some(2000));
    }

    // Two project records share ts=1500; the later one in input order wins
    // as "latest" (identified reads 99/111) and orders second as a snapshot
    // (baseline 10000 from the first, current 9000 from the second).
    // Assumed mode, now = 1500 + 86400, rate 10/day:
    // Interval [1500, 1500): zero length -> 0.
    // Interval [1500, 87900): (10000 - 9000) * 10 * 1 day = 10000.
    // synthetic count = 10 * 86400 / 86400 = 10.
    #[test]
    fn ts_tie_latest_input_record_wins() {
        let ledger = vec![
            rec(
                "project",
                "/p",
                1500,
                json!({ "always_tokens": 10_000, "reclaimable_min": 1, "reclaimable_max": 2 }),
            ),
            rec(
                "project",
                "/p",
                1500,
                json!({ "always_tokens": 9_000, "reclaimable_min": 99, "reclaimable_max": 111 }),
            ),
        ];
        let mut tier_inputs = inputs(&ledger, None);
        tier_inputs.now = 1500 + 86_400;
        let report = compute(&tier_inputs);
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.project_reclaimable_min, Some(99));
        assert_eq!(identified.project_reclaimable_max, Some(111));
        assert_eq!(identified.project_as_of, Some(1500));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.baseline_always_tokens, 10_000);
        assert_eq!(realized.current_always_tokens, 9_000);
        assert_eq!(realized.tokens, 10_000);
        assert_eq!(realized.sessions_count, 10);
    }

    // The t=2000 project record has no always_tokens: not a snapshot, so the
    // snapshot sequence is 10000 at t=1000 then 8000 at t=3000.
    // Session 2500: w_active 10000 (the t=3000 snapshot is not standing
    // yet) -> 0. Session 3500: w_active 8000 -> +2000.
    // realized = 2000, sessions_count = 2.
    // Identified still uses the LATEST project record (t=3000), whose
    // missing reclaimable fields default to 0 defensively.
    #[test]
    fn project_record_missing_always_tokens_is_not_a_snapshot() {
        let ledger = vec![
            snap("/p", 1000, 10_000),
            rec(
                "project",
                "/p",
                2000,
                json!({ "reclaimable_min": 5, "reclaimable_max": 9 }),
            ),
            snap("/p", 3000, 8_000),
        ];
        let sessions = vec![
            session("/p", 2500, UsageSource::ClaudeCode),
            session("/p", 3500, UsageSource::ClaudeCode),
        ];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let realized = report.realized.expect("realized present");
        assert_eq!(realized.tokens, 2000);
        assert_eq!(realized.sessions_count, 2);
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.project_reclaimable_min, Some(0));
        assert_eq!(identified.project_reclaimable_max, Some(0));
        assert_eq!(identified.project_as_of, Some(3000));
    }

    // scope_project = "/a": /b's audit record and /b's session are both
    // invisible. Identified carries only /a's audit; measured counts only
    // /a's session.
    #[test]
    fn project_scope_filters_other_projects() {
        let ledger = vec![
            rec(
                "audit",
                "/a",
                1000,
                json!({ "savings_min": 10, "savings_max": 20 }),
            ),
            rec(
                "audit",
                "/b",
                2000,
                json!({ "savings_min": 999, "savings_max": 1000 }),
            ),
        ];
        let sessions = vec![
            session("/a", 1100, UsageSource::ClaudeCode),
            session("/b", 1200, UsageSource::ClaudeCode),
        ];
        let mut tier_inputs = inputs(&ledger, Some(&sessions));
        tier_inputs.scope_project = Some("/a");
        let report = compute(&tier_inputs);
        let identified = report.identified.expect("identified present");
        assert_eq!(identified.audit_savings_min, Some(10));
        assert_eq!(identified.audit_savings_max, Some(20));
        assert_eq!(identified.audit_as_of, Some(1000));
        assert_eq!(identified.projects, 1);
        assert!(report.realized.is_none());
        let measured = report.measured.expect("measured present");
        assert_eq!(measured.sessions, 1);
        assert_eq!(measured.first_ts, Some(1100));
    }

    // Serialization smoke test: tiers appear under their keys with the
    // binding labels, and absent tiers serialize as null (stable schema for
    // --json consumers).
    #[test]
    fn report_serializes_with_tier_labels() {
        let ledger = vec![snap("/p", 1000, 10_000), snap("/p", 2000, 8_000)];
        let sessions = vec![session("/p", 2500, UsageSource::ClaudeCode)];
        let report = compute(&inputs(&ledger, Some(&sessions)));
        let value = serde_json::to_value(&report).expect("serializes");
        assert_eq!(value["identified"]["label"], "advisory estimate");
        assert_eq!(
            value["realized"]["label"],
            "measured structure, estimated frequency"
        );
        assert_eq!(value["measured"]["label"], "ground truth");
        assert_eq!(value["realized"]["tokens"], 2000);
        assert!(value["notes"].as_array().is_some_and(|n| !n.is_empty()));

        let empty: Vec<LedgerRecord> = Vec::new();
        let empty_report = compute(&inputs(&empty, None));
        let empty_value = serde_json::to_value(&empty_report).expect("serializes");
        assert!(empty_value["identified"].is_null());
        assert!(empty_value["realized"].is_null());
        assert!(empty_value["measured"].is_null());
    }
}
