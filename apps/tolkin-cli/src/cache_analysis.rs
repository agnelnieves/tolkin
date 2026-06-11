//! Measured prompt-cache health analysis (the `tolkin cache` engine).
//!
//! Pure module: no IO, no environment access, no clock reads. Everything
//! arrives through [`CacheInputs`]: the per-request tuples retained on each
//! ingested session plus an injected per-model rate lookup backed by the
//! pricing table. Every number this module emits carries one of the binding
//! tier labels:
//!
//! - "ground truth" (Tier 3, measured): hit rate, write churn, cadence
//!   facts, and the OBSERVED cache-write volumes. These are sums and ratios
//!   over what the logs actually recorded.
//! - "advisory estimate" (Tier 1, identified): every TTL-counterfactual
//!   number (simulated write events, simulated write tokens, and the dollar
//!   delta). They are computed FROM ground-truth gap data, but a simulated
//!   strategy never ran, so the outputs are estimates and say so.
//!
//! # Source scope
//!
//! Cache analysis is sourced from Claude Code sessions only. Codex rollout
//! events carry fresh and cached input but none of the cache-write fields
//! the churn and TTL analyses are built on (OpenAI's automatic caching
//! exposes no write events and no TTL choice), so Codex sessions retain no
//! request tuples (usage::types::SessionUsage) and are filtered out here.
//!
//! # Dedup inheritance
//!
//! Tuples are retained AFTER the global cross-file dedup in
//! `usage::claude_code::aggregate_records`, so a resumed session's copied
//! records feed the analysis exactly once. Attribution follows the I2
//! winner rule: identical copies stay with the lexicographically first file,
//! which can split one logical resumed session across two SessionUsage rows.
//! Totals never double count; gap classification inherits that split.
//!
//! # Determinism contract
//!
//! The report is a pure function of the input multiset: sessions are
//! re-sorted internally, per-session requests are re-sorted by the full
//! tuple, dollar sums aggregate per model id in BTreeMap (sorted) order, and
//! every ratio is a single f64 division of exact integer sums. An
//! independent recompute that follows this module's rustdoc must match
//! field for field.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::usage::claude_code::utc_day_from_secs;
use crate::usage::types::{RequestUsage, SessionUsage, UsageSource};

/// Tier labels, verbatim from `tiers.rs` (kept as literals so this module
/// stays dependency-light; a unit test pins them against `tiers`).
pub const LABEL_MEASURED: &str = "ground truth";
pub const LABEL_IDENTIFIED: &str = "advisory estimate";

/// Hit rate below this on any active day triggers the broken-cache
/// advisory. Threshold source: independent agent-workload research (2026)
/// puts the operational rule at "a cache hit rate under 0.5 over a normal
/// hour of agent work means caching is effectively broken"; a normal agent
/// workload sits well above 0.5 because the growing conversation prefix
/// re-reads on every turn.
pub const HIT_RATE_BROKEN_THRESHOLD: f64 = 0.5;

/// The two Anthropic cache TTLs, in seconds.
const TTL_5M_SECS: u64 = 300;
const TTL_1H_SECS: u64 = 3600;

/// Marginal cache-write cost multipliers over the base input rate, used for
/// the break-even statement and the weighted write-token comparison. A 5m
/// write bills 1.25x input but replaces a 0.1x cached read (marginal 1.15);
/// a 1h write bills 2x minus the same 0.1x (marginal 1.9). These are the
/// uniform Anthropic multipliers; `usd` figures are derived per model from
/// the pricing table instead, and a test asserts the table agrees with
/// these constants for every Anthropic model.
pub const MARGINAL_MULTIPLIER_5M: f64 = 1.15;
pub const MARGINAL_MULTIPLIER_1H: f64 = 1.9;

/// Derived from the multipliers: the 1h TTL wins exactly when
/// 1.9 x W1 < 1.15 x W5, that is when W1/W5 is under about 0.6.
pub const BREAK_EVEN: &str = "the 1h TTL wins exactly when 1.9 x W1 < 1.15 x W5 (equivalently when W1/W5 is under about 0.6)";

/// The scope line, printed always, on every surface that shows this report.
pub const SCOPE_LINE: &str = "Claude Code manages its own caching, so for Claude Code sessions the actionable levers are prefix stability and session shape; TTL choice is a direct lever only for API pipeline builders.";

/// Source disclosure, printed with the report.
pub const SOURCE_NOTE: &str =
    "Cache analysis covers Claude Code session logs only; Codex rollouts carry no cache-write fields.";

/// Printed with the churn figures. Claude Code extends its conversation
/// prefix on most turns, so each turn's newly appended suffix lands in
/// `writes_after_first_tokens` by construction; a high share on agent
/// transcripts reflects conversation growth as well as instability. The
/// number is still measured truth (every counted write DID change the
/// prefix); this note keeps it from being misread as a defect score.
pub const CHURN_NOTE: &str = "Write churn counts every cache write after a session's first write as a changed prefix. Claude Code appends to its conversation prefix on most turns, so growth-driven suffix writes dominate this share by construction; read it comparatively across sessions, not as a defect score.";

/// How many churn offenders to name.
const WORST_SESSIONS_CAP: usize = 5;

/// Per-model cache rates in USD per million tokens, resolved by the caller
/// from the pricing table (with the same fallbacks the cost path uses).
/// `None` from the rate fn marks the model unpriced. The marginal write
/// cost needs exactly these three rates: write rate minus cache-read rate;
/// the base input rate enters only through the published multipliers (see
/// [`MARGINAL_MULTIPLIER_5M`] and the test pinning them to the table).
#[derive(Clone, Copy, Debug)]
pub struct ModelRates {
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
}

/// Inputs for [`analyze`]. The module never reads the environment or the
/// clock; the caller supplies everything, keeping the report reproducible
/// from its inputs.
pub struct CacheInputs<'a> {
    /// `None` = global scope (all projects).
    pub scope_project: Option<&'a str>,
    /// All ingested sessions; the analysis filters to Claude Code itself.
    pub sessions: &'a [SessionUsage],
    /// Pricing lookup, called with the RAW model id as logged.
    pub rate_fn: &'a dyn Fn(&str) -> Option<ModelRates>,
}

#[derive(Debug, Serialize)]
pub struct CacheReport {
    /// Always "claude_code" today; see the module docs for why.
    pub source: &'static str,
    pub sessions_analyzed: u64,
    pub requests_analyzed: u64,
    pub hit_rate: HitRate,
    pub write_churn: WriteChurn,
    pub ttl_counterfactual: TtlCounterfactual,
    pub cadence: Cadence,
    /// [`SCOPE_LINE`], serialized so every JSON consumer carries it too.
    pub scope_line: &'static str,
    pub notes: Vec<String>,
}

/// Cache hit rate over the scope plus the per-day broken-cache check.
/// Tier: measured ("ground truth").
#[derive(Debug, Serialize)]
pub struct HitRate {
    pub label: &'static str,
    /// cache_read over input-side tokens; 0.0 when the denominator is 0.
    pub rate: f64,
    pub cache_read_tokens: u64,
    pub input_side_tokens: u64,
    pub threshold: f64,
    /// Active UTC days (input-side tokens > 0) whose rate fell under the
    /// threshold, ascending by day.
    pub days_below_threshold: Vec<DayRate>,
    /// Present exactly when `days_below_threshold` is non-empty.
    pub advisory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DayRate {
    pub day: String,
    pub rate: f64,
    pub input_side_tokens: u64,
}

/// Cache writes AFTER a session's first write mean the prefix changed
/// mid-session: prefix instability caught in the act. Tier: measured
/// ("ground truth").
#[derive(Debug, Serialize)]
pub struct WriteChurn {
    pub label: &'static str,
    pub total_write_tokens: u64,
    pub writes_after_first_tokens: u64,
    /// writes_after_first over total writes; 0.0 when no writes.
    pub share: f64,
    pub sessions_with_writes: u64,
    /// Top offenders by tokens written after the first write, descending;
    /// ties by earlier first_ts then session_id. Session id and start
    /// timestamp only, never content.
    pub worst_sessions: Vec<ChurnSession>,
}

#[derive(Debug, Serialize)]
pub struct ChurnSession {
    pub session_id: String,
    pub first_ts: u64,
    pub write_tokens: u64,
    pub writes_after_first_tokens: u64,
    pub share: f64,
}

/// The 5m-vs-1h TTL counterfactual over the real gap timeline.
///
/// Observed volumes are Tier 3 ground truth; every simulated number and
/// every dollar figure is a Tier 1 advisory estimate computed from those
/// ground-truth inputs (a simulated strategy never ran).
#[derive(Debug, Serialize)]
pub struct TtlCounterfactual {
    pub label: &'static str,
    pub inputs_label: &'static str,
    pub tier_note: String,
    /// Ground truth: observed cache-write volumes by TTL.
    pub observed_write_tokens_5m: u64,
    pub observed_write_tokens_1h: u64,
    /// Sessions simulated: those with at least one observed cache write
    /// (the observed first-write size is the prefix proxy, so a session
    /// with no writes has no observable prefix to simulate).
    pub sessions_simulated: u64,
    pub sessions_without_writes_excluded: u64,
    /// Advisory estimates: simulated write events and tokens per strategy.
    pub simulated_w5_write_events: u64,
    pub simulated_w1_write_events: u64,
    pub simulated_w5_write_tokens: u64,
    pub simulated_w1_write_tokens: u64,
    /// What multiplies the events into tokens.
    pub prefix_proxy: &'static str,
    pub marginal_multiplier_5m: f64,
    pub marginal_multiplier_1h: f64,
    /// Advisory estimates: marginal dollars per strategy at per-model rates
    /// (write rate minus cache-read rate, from the pricing table), priced
    /// models only.
    pub usd_5m_strategy: f64,
    pub usd_1h_strategy: f64,
    pub usd_delta_1h_minus_5m: f64,
    /// Share of simulated write tokens on priced models (1.0 when all are
    /// priced, 0.0 when nothing is priced or nothing was simulated).
    pub priced_share_of_simulated_tokens: f64,
    /// Models excluded from the dollar figures, sorted.
    pub unpriced_models: Vec<String>,
    pub break_even: &'static str,
    pub verdict: String,
}

/// Timeline facts that make the TTL advice concrete. Tier: measured
/// ("ground truth").
#[derive(Debug, Serialize)]
pub struct Cadence {
    pub label: &'static str,
    pub intra_session_gaps: u64,
    pub intra_session_gaps_over_5m: u64,
    pub share_intra_gaps_over_5m: f64,
    /// Inter-session gaps are measured between consecutive sessions OF THE
    /// SAME PROJECT (next session's first request minus previous session's
    /// last request), because a surviving cache implies a shared prefix and
    /// unrelated projects share none. Global scope aggregates the
    /// per-project gap sets. Subagent streams (Task-tool fan-out) live in
    /// the sessions vector alongside parent sessions and so contribute their
    /// own pair-wise gaps within their project; each subagent has its own
    /// independent prefix, so the gap between a parent and one of its
    /// subagents is not a "could the cache have survived" gap in any
    /// strict sense, but is reported as-is for the project's overall
    /// timeline shape. Read the inter-session distribution as the
    /// project's cadence (TTL counterfactual already handles each stream's
    /// intra-session math separately).
    pub inter_session_gaps: u64,
    pub inter_session_gaps_under_1h: u64,
    pub share_inter_gaps_under_1h: f64,
    pub sessions_zero_cache_read: u64,
}

/// Compute the cache health report. See the module docs for scope, tier
/// labeling, and the determinism contract.
pub fn analyze(inputs: &CacheInputs) -> CacheReport {
    // Filter: Claude Code only, scope only. Sort the working set by
    // (project_key, first_ts, last_ts, session_id) so every downstream
    // ordering is independent of caller order.
    let mut sessions: Vec<&SessionUsage> = inputs
        .sessions
        .iter()
        .filter(|s| matches!(s.source, UsageSource::ClaudeCode))
        .filter(|s| match inputs.scope_project {
            Some(scope) => s.project_key == scope,
            None => true,
        })
        .collect();
    sessions.sort_by(|a, b| {
        (a.project_key.as_str(), a.first_ts, a.last_ts, &a.session_id).cmp(&(
            b.project_key.as_str(),
            b.first_ts,
            b.last_ts,
            &b.session_id,
        ))
    });

    // Re-sort each session's requests defensively (the readers already sort,
    // but this module must not trust its callers for its own determinism).
    let ordered: Vec<(usize, Vec<RequestUsage>)> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut reqs = s.requests.clone();
            reqs.sort_unstable();
            (i, reqs)
        })
        .collect();

    let requests_analyzed: u64 = ordered.iter().map(|(_, r)| r.len() as u64).sum();

    let hit_rate = compute_hit_rate(&ordered);
    let write_churn = compute_write_churn(&sessions, &ordered);
    let ttl_counterfactual = compute_ttl_counterfactual(&sessions, &ordered, inputs.rate_fn);
    let cadence = compute_cadence(&sessions, &ordered);

    let notes = vec![
        SOURCE_NOTE.to_string(),
        CHURN_NOTE.to_string(),
        "All savings are input-token bounded; output may vary.".to_string(),
    ];

    CacheReport {
        source: "claude_code",
        sessions_analyzed: sessions.len() as u64,
        requests_analyzed,
        hit_rate,
        write_churn,
        ttl_counterfactual,
        cadence,
        scope_line: SCOPE_LINE,
        notes,
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn compute_hit_rate(ordered: &[(usize, Vec<RequestUsage>)]) -> HitRate {
    let mut read: u64 = 0;
    let mut input_side: u64 = 0;
    // Day key -> (cache_read, input_side). BTreeMap iteration gives the
    // ascending day order the output promises.
    let mut by_day: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (_, reqs) in ordered {
        for r in reqs {
            read += r.cache_read_tokens;
            input_side += r.input_side();
            let day = by_day.entry(utc_day_from_secs(r.ts)).or_insert((0, 0));
            day.0 += r.cache_read_tokens;
            day.1 += r.input_side();
        }
    }
    let days_below_threshold: Vec<DayRate> = by_day
        .into_iter()
        .filter(|(_, (_, day_input_side))| *day_input_side > 0)
        .map(|(day, (day_read, day_input_side))| DayRate {
            day,
            rate: ratio(day_read, day_input_side),
            input_side_tokens: day_input_side,
        })
        .filter(|d| d.rate < HIT_RATE_BROKEN_THRESHOLD)
        .collect();
    let advisory = if days_below_threshold.is_empty() {
        None
    } else {
        let worst = days_below_threshold
            .iter()
            .min_by(|a, b| a.rate.total_cmp(&b.rate))
            .expect("non-empty");
        Some(format!(
            "caching is likely broken: the cache hit rate fell under {:.2} on {} active day(s) (worst {} at {:.1}%). A normal agent workload sits well above 0.5; a persistently low rate usually means a volatile prompt prefix or caching not engaging at all.",
            HIT_RATE_BROKEN_THRESHOLD,
            days_below_threshold.len(),
            worst.day,
            worst.rate * 100.0,
        ))
    };
    HitRate {
        label: LABEL_MEASURED,
        rate: ratio(read, input_side),
        cache_read_tokens: read,
        input_side_tokens: input_side,
        threshold: HIT_RATE_BROKEN_THRESHOLD,
        days_below_threshold,
        advisory,
    }
}

/// Per-session churn split: (total write tokens, write tokens after the
/// first write-bearing request, the first write's size). Requests must be
/// sorted; the "first write" is the first request in tuple order with
/// write tokens > 0, so equal timestamps resolve deterministically.
fn churn_split(reqs: &[RequestUsage]) -> (u64, u64, u64) {
    let total: u64 = reqs.iter().map(|r| r.write_tokens()).sum();
    let first = reqs
        .iter()
        .map(|r| r.write_tokens())
        .find(|w| *w > 0)
        .unwrap_or(0);
    (total, total - first, first)
}

fn compute_write_churn(
    sessions: &[&SessionUsage],
    ordered: &[(usize, Vec<RequestUsage>)],
) -> WriteChurn {
    let mut total_write_tokens: u64 = 0;
    let mut writes_after_first_tokens: u64 = 0;
    let mut sessions_with_writes: u64 = 0;
    let mut offenders: Vec<ChurnSession> = Vec::new();
    for (idx, reqs) in ordered {
        let (total, after_first, _) = churn_split(reqs);
        if total == 0 {
            continue;
        }
        sessions_with_writes += 1;
        total_write_tokens += total;
        writes_after_first_tokens += after_first;
        if after_first > 0 {
            let s = sessions[*idx];
            offenders.push(ChurnSession {
                session_id: s.session_id.clone(),
                first_ts: s.first_ts,
                write_tokens: total,
                writes_after_first_tokens: after_first,
                share: ratio(after_first, total),
            });
        }
    }
    offenders.sort_by(|a, b| {
        b.writes_after_first_tokens
            .cmp(&a.writes_after_first_tokens)
            .then(a.first_ts.cmp(&b.first_ts))
            .then(a.session_id.cmp(&b.session_id))
    });
    offenders.truncate(WORST_SESSIONS_CAP);
    WriteChurn {
        label: LABEL_MEASURED,
        total_write_tokens,
        writes_after_first_tokens,
        share: ratio(writes_after_first_tokens, total_write_tokens),
        sessions_with_writes,
        worst_sessions: offenders,
    }
}

/// Simulated cache-write events for one session's sorted request timeline
/// under a stable prefix and the given TTL, with READS REFRESHING THE TTL:
/// every request that lands inside the live window resets the expiry clock,
/// so the simulation re-writes exactly when the gap between two consecutive
/// requests STRICTLY exceeds the TTL (a gap equal to the TTL is still a
/// hit). The first request always writes.
///
/// Refresh-semantics citation (pinned 2026-06-10 by the A7 review):
///
/// 1. Anthropic Platform prompt caching docs
///    (https://platform.claude.com/docs/en/build-with-claude/prompt-caching;
///    formerly docs.anthropic.com, 301-redirected to platform.claude.com on
///    fetch). The "How prompt caching works" section states verbatim: "By
///    default, the cache has a 5-minute lifetime. The cache is refreshed
///    for no additional cost each time the cached content is used." The
///    1-hour TTL is documented as the same mechanism with a longer
///    lifetime ("If you find that 5 minutes is too short, Anthropic also
///    offers a 1-hour cache duration at additional cost"), and the
///    multi-turn section uses the phrase "cache refresh" for hits within
///    the live window.
/// 2. AWS Bedrock prompt caching docs make the dual-TTL claim explicit
///    where the Anthropic page is general
///    (https://docs.aws.amazon.com/bedrock/latest/userguide/prompt-caching.html):
///    "The cache has a Time To Live (TTL), which resets with each
///    successful cache hit. During this period, the context in the cache
///    is preserved. If no cache hits occur within the TTL window, your
///    cache expires." This sentence sits above the table that lists the
///    same models as supporting "5 minutes, 1 hour", so it covers both
///    TTLs on the same models tolkin's pricing table prices.
///
/// Caveat captured in the surfaces: the Anthropic doc does not single out
/// the 1-hour TTL with its own refresh sentence, so the dual-TTL claim
/// leans on the general statement plus the Bedrock corroboration. If
/// Anthropic ever publishes 1h-specific refresh wording that differs from
/// 5m, this function and the verdict text must change together.
fn simulated_write_events_reads_refresh_ttl(sorted_ts: &[u64], ttl_secs: u64) -> u64 {
    if sorted_ts.is_empty() {
        return 0;
    }
    let mut events: u64 = 1; // the first request writes the prefix
    for pair in sorted_ts.windows(2) {
        let gap = pair[1].saturating_sub(pair[0]);
        if gap > ttl_secs {
            events += 1;
        }
    }
    events
}

/// The session's dominant model: maximum input-side volume in `by_model`,
/// ties broken by the lexicographically smallest raw id (BTreeMap iteration
/// is ascending, and a strictly-greater comparison keeps the first seen).
fn dominant_model(session: &SessionUsage) -> Option<&str> {
    let mut best: Option<(&str, u64)> = None;
    for (model, totals) in &session.by_model {
        let volume = totals.input_side();
        match best {
            Some((_, best_volume)) if volume <= best_volume => {}
            _ => best = Some((model.as_str(), volume)),
        }
    }
    best.map(|(model, _)| model)
}

fn compute_ttl_counterfactual(
    sessions: &[&SessionUsage],
    ordered: &[(usize, Vec<RequestUsage>)],
    rate_fn: &dyn Fn(&str) -> Option<ModelRates>,
) -> TtlCounterfactual {
    let mut observed_5m: u64 = 0;
    let mut observed_1h: u64 = 0;
    let mut sessions_simulated: u64 = 0;
    let mut sessions_excluded: u64 = 0;
    let mut w5_events: u64 = 0;
    let mut w1_events: u64 = 0;
    let mut w5_tokens: u64 = 0;
    let mut w1_tokens: u64 = 0;
    // Per-model simulated token volumes for the dollar conversion, keyed by
    // the RAW dominant model id (sorted iteration keeps the sum order
    // deterministic for an exact independent recompute).
    let mut by_model: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for (idx, reqs) in ordered {
        for r in reqs {
            observed_5m += r.cache_write_5m_tokens;
            observed_1h += r.cache_write_1h_tokens;
        }
        let (total_writes, _, first_write) = churn_split(reqs);
        if total_writes == 0 {
            // No observed write means no observable prefix size to simulate.
            if !reqs.is_empty() {
                sessions_excluded += 1;
            }
            continue;
        }
        sessions_simulated += 1;
        let ts: Vec<u64> = reqs.iter().map(|r| r.ts).collect();
        let session_w5 = simulated_write_events_reads_refresh_ttl(&ts, TTL_5M_SECS);
        let session_w1 = simulated_write_events_reads_refresh_ttl(&ts, TTL_1H_SECS);
        let session_w5_tokens = session_w5.saturating_mul(first_write);
        let session_w1_tokens = session_w1.saturating_mul(first_write);
        w5_events += session_w5;
        w1_events += session_w1;
        w5_tokens += session_w5_tokens;
        w1_tokens += session_w1_tokens;
        let model = dominant_model(sessions[*idx]).unwrap_or("").to_string();
        let entry = by_model.entry(model).or_insert((0, 0));
        entry.0 += session_w5_tokens;
        entry.1 += session_w1_tokens;
    }

    // Dollars: marginal write rate = write rate minus cache-read rate, per
    // model from the pricing table, summed in sorted model-id order.
    let mut usd_5m = 0.0f64;
    let mut usd_1h = 0.0f64;
    let mut priced_tokens: u64 = 0;
    let mut unpriced_tokens: u64 = 0;
    let mut unpriced_models: Vec<String> = Vec::new();
    for (model, (model_w5_tokens, model_w1_tokens)) in &by_model {
        match rate_fn(model) {
            Some(rates) => {
                usd_5m += *model_w5_tokens as f64 * (rates.cache_write_5m - rates.cache_read) / 1e6;
                usd_1h += *model_w1_tokens as f64 * (rates.cache_write_1h - rates.cache_read) / 1e6;
                priced_tokens += model_w5_tokens + model_w1_tokens;
            }
            None => {
                unpriced_tokens += model_w5_tokens + model_w1_tokens;
                unpriced_models.push(model.clone());
            }
        }
    }
    let priced_share = ratio(priced_tokens, priced_tokens + unpriced_tokens);

    // The verdict compares the strategies at the uniform Anthropic marginal
    // multipliers (model-rate independent), then quotes the priced dollars.
    let weighted_w5 = MARGINAL_MULTIPLIER_5M * w5_tokens as f64;
    let weighted_w1 = MARGINAL_MULTIPLIER_1H * w1_tokens as f64;
    let verdict = if sessions_simulated == 0 {
        "advisory estimate: no sessions with observed cache writes, so there is no prefix to simulate.".to_string()
    } else {
        let winner = if weighted_w1 < weighted_w5 {
            "the 1h TTL strategy is cheaper"
        } else if weighted_w1 > weighted_w5 {
            "the 5m TTL strategy is cheaper"
        } else {
            "the two TTL strategies tie"
        };
        format!(
            "advisory estimate: on this real gap timeline {winner} for a stable prefix (1.9 x W1 = {:.0} vs 1.15 x W5 = {:.0} marginal write tokens; priced-model dollars: 1h ${:.2} vs 5m ${:.2}).",
            weighted_w1, weighted_w5, usd_1h, usd_5m,
        )
    };

    TtlCounterfactual {
        label: LABEL_IDENTIFIED,
        inputs_label: LABEL_MEASURED,
        tier_note: format!(
            "Gap data and observed write volumes are {LABEL_MEASURED}; every counterfactual number (simulated events, simulated tokens, dollars) is an {LABEL_IDENTIFIED} computed from those ground-truth inputs.",
        ),
        observed_write_tokens_5m: observed_5m,
        observed_write_tokens_1h: observed_1h,
        sessions_simulated,
        sessions_without_writes_excluded: sessions_excluded,
        simulated_w5_write_events: w5_events,
        simulated_w1_write_events: w1_events,
        simulated_w5_write_tokens: w5_tokens,
        simulated_w1_write_tokens: w1_tokens,
        prefix_proxy:
            "each session's observed first-write token size stands in for the stable prefix",
        marginal_multiplier_5m: MARGINAL_MULTIPLIER_5M,
        marginal_multiplier_1h: MARGINAL_MULTIPLIER_1H,
        usd_5m_strategy: usd_5m,
        usd_1h_strategy: usd_1h,
        usd_delta_1h_minus_5m: usd_1h - usd_5m,
        priced_share_of_simulated_tokens: priced_share,
        unpriced_models,
        break_even: BREAK_EVEN,
        verdict,
    }
}

fn compute_cadence(sessions: &[&SessionUsage], ordered: &[(usize, Vec<RequestUsage>)]) -> Cadence {
    let mut intra_gaps: u64 = 0;
    let mut intra_over_5m: u64 = 0;
    let mut zero_read_sessions: u64 = 0;
    for (_, reqs) in ordered {
        for pair in reqs.windows(2) {
            intra_gaps += 1;
            if pair[1].ts.saturating_sub(pair[0].ts) > TTL_5M_SECS {
                intra_over_5m += 1;
            }
        }
        if !reqs.is_empty() && reqs.iter().all(|r| r.cache_read_tokens == 0) {
            zero_read_sessions += 1;
        }
    }

    // Inter-session gaps, per project. `sessions` arrives sorted by
    // (project_key, first_ts, last_ts, session_id), so consecutive entries
    // of the same project are already in timeline order.
    let mut inter_gaps: u64 = 0;
    let mut inter_under_1h: u64 = 0;
    for pair in sessions.windows(2) {
        if pair[0].project_key != pair[1].project_key {
            continue;
        }
        inter_gaps += 1;
        // Overlapping sessions saturate to a zero gap, which is under an
        // hour by definition.
        if pair[1].first_ts.saturating_sub(pair[0].last_ts) < TTL_1H_SECS {
            inter_under_1h += 1;
        }
    }

    Cadence {
        label: LABEL_MEASURED,
        intra_session_gaps: intra_gaps,
        intra_session_gaps_over_5m: intra_over_5m,
        share_intra_gaps_over_5m: ratio(intra_over_5m, intra_gaps),
        inter_session_gaps: inter_gaps,
        inter_session_gaps_under_1h: inter_under_1h,
        share_inter_gaps_under_1h: ratio(inter_under_1h, inter_gaps),
        sessions_zero_cache_read: zero_read_sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::UsageTotals;
    use std::collections::BTreeMap as Map;

    fn req(ts: u64, input: u64, read: u64, w5: u64, w1: u64) -> RequestUsage {
        RequestUsage {
            ts,
            input_tokens: input,
            cache_read_tokens: read,
            cache_write_5m_tokens: w5,
            cache_write_1h_tokens: w1,
        }
    }

    /// Build a Claude Code session whose totals/by_model are derived from
    /// its requests (the same arithmetic the reader performs).
    fn session(id: &str, project: &str, model: &str, requests: Vec<RequestUsage>) -> SessionUsage {
        let mut totals = UsageTotals::default();
        for r in &requests {
            totals.input_tokens += r.input_tokens;
            totals.cache_read_tokens += r.cache_read_tokens;
            totals.cache_write_5m_tokens += r.cache_write_5m_tokens;
            totals.cache_write_1h_tokens += r.cache_write_1h_tokens;
        }
        let first_ts = requests.iter().map(|r| r.ts).min().unwrap_or(0);
        let last_ts = requests.iter().map(|r| r.ts).max().unwrap_or(0);
        let mut by_model = Map::new();
        by_model.insert(model.to_string(), totals);
        SessionUsage {
            source: UsageSource::ClaudeCode,
            session_id: id.to_string(),
            project_key: project.to_string(),
            first_ts,
            last_ts,
            totals,
            by_model,
            by_day: Map::new(),
            requests,
        }
    }

    fn codex_session(id: &str, project: &str) -> SessionUsage {
        SessionUsage {
            source: UsageSource::Codex,
            session_id: id.to_string(),
            project_key: project.to_string(),
            first_ts: 10,
            last_ts: 20,
            totals: UsageTotals {
                input_tokens: 5,
                output_tokens: 5,
                cache_read_tokens: 100,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
            },
            by_model: Map::new(),
            by_day: Map::new(),
            requests: Vec::new(),
        }
    }

    /// Sonnet 4.6 published cache rates (read 0.30, w5 3.75, w1 6.0).
    fn sonnet_rates(model: &str) -> Option<ModelRates> {
        (model == "claude-sonnet-4-6").then_some(ModelRates {
            cache_read: 0.30,
            cache_write_5m: 3.75,
            cache_write_1h: 6.0,
        })
    }

    fn no_rates(_model: &str) -> Option<ModelRates> {
        None
    }

    fn analyze_sessions(sessions: &[SessionUsage]) -> CacheReport {
        analyze(&CacheInputs {
            scope_project: None,
            sessions,
            rate_fn: &sonnet_rates,
        })
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // Single session, simple gaps. Requests at t = 0, 100, 700, 5000:
    // gaps 100, 600, 4300. First request writes 1000 (1h).
    // W5 events = 1 + (600>300) + (4300>300) = 3; tokens = 3000.
    // W1 events = 1 + (4300>3600) = 2; tokens = 2000.
    // Hit rate: read 600 over input_side (10+200) + (10+200) + (10+200) +
    // (10+0) + writes 1000 = 1640... computed below from the fixture.
    #[test]
    fn golden_single_session_simple_gaps() {
        let sessions = vec![session(
            "s1",
            "/p",
            "claude-sonnet-4-6",
            vec![
                req(0, 10, 0, 0, 1000),
                req(100, 10, 200, 0, 0),
                req(700, 10, 200, 0, 0),
                req(5000, 10, 200, 0, 0),
            ],
        )];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.sessions_analyzed, 1);
        assert_eq!(report.requests_analyzed, 4);
        let ttl = &report.ttl_counterfactual;
        assert_eq!(ttl.simulated_w5_write_events, 3);
        assert_eq!(ttl.simulated_w1_write_events, 2);
        assert_eq!(ttl.simulated_w5_write_tokens, 3000);
        assert_eq!(ttl.simulated_w1_write_tokens, 2000);
        assert_eq!(ttl.observed_write_tokens_5m, 0);
        assert_eq!(ttl.observed_write_tokens_1h, 1000);
        assert_eq!(ttl.sessions_simulated, 1);
        assert_eq!(ttl.sessions_without_writes_excluded, 0);
        // Dollars at sonnet rates: marginal 5m = 3.75 - 0.30 = 3.45/MTok,
        // marginal 1h = 6.0 - 0.30 = 5.70/MTok.
        // usd_5m = 3000 * 3.45 / 1e6 = 0.01035.
        // usd_1h = 2000 * 5.70 / 1e6 = 0.0114.
        assert!(
            approx(ttl.usd_5m_strategy, 0.01035),
            "{}",
            ttl.usd_5m_strategy
        );
        assert!(
            approx(ttl.usd_1h_strategy, 0.0114),
            "{}",
            ttl.usd_1h_strategy
        );
        assert!(approx(ttl.usd_delta_1h_minus_5m, 0.0114 - 0.01035));
        assert!(approx(ttl.priced_share_of_simulated_tokens, 1.0));
        assert!(ttl.unpriced_models.is_empty());
        // Hit rate: read = 600; input_side = 4 x 10 + 600 + 1000 = 1640.
        let hr = &report.hit_rate;
        assert_eq!(hr.cache_read_tokens, 600);
        assert_eq!(hr.input_side_tokens, 1640);
        assert!(approx(hr.rate, 600.0 / 1640.0));
        // 600/1640 = 0.3659 < 0.5 on the single active day: advisory fires.
        assert_eq!(hr.days_below_threshold.len(), 1);
        assert!(hr
            .advisory
            .as_deref()
            .unwrap_or("")
            .contains("caching is likely broken"));
        // Churn: one write only, nothing after the first.
        assert_eq!(report.write_churn.total_write_tokens, 1000);
        assert_eq!(report.write_churn.writes_after_first_tokens, 0);
        assert!(approx(report.write_churn.share, 0.0));
        assert!(report.write_churn.worst_sessions.is_empty());
        // Cadence: 3 intra gaps, 2 over 5m (600, 4300); no inter gaps.
        assert_eq!(report.cadence.intra_session_gaps, 3);
        assert_eq!(report.cadence.intra_session_gaps_over_5m, 2);
        assert!(approx(report.cadence.share_intra_gaps_over_5m, 2.0 / 3.0));
        assert_eq!(report.cadence.inter_session_gaps, 0);
        assert!(approx(report.cadence.share_inter_gaps_under_1h, 0.0));
        assert_eq!(report.cadence.sessions_zero_cache_read, 0);
        // Labels, verbatim.
        assert_eq!(report.hit_rate.label, "ground truth");
        assert_eq!(report.write_churn.label, "ground truth");
        assert_eq!(report.cadence.label, "ground truth");
        assert_eq!(report.ttl_counterfactual.label, "advisory estimate");
        assert_eq!(report.ttl_counterfactual.inputs_label, "ground truth");
        assert_eq!(report.scope_line, SCOPE_LINE);
    }

    // Boundary pinning: a gap of EXACTLY the TTL is a hit (strict
    // inequality); one second more expires.
    #[test]
    fn golden_gaps_straddling_exact_ttl_boundaries() {
        // Gaps: 300 (hit at 5m), 301 (miss at 5m), 3600 (hit at 1h after
        // refresh), 3601 (miss at 1h).
        let ts = [0u64, 300, 601, 4201, 7802];
        assert_eq!(simulated_write_events_reads_refresh_ttl(&ts, 300), 4); // 1 + misses at 301, 3600, 3601
        assert_eq!(simulated_write_events_reads_refresh_ttl(&ts, 3600), 2); // 1 + miss at 3601
        let sessions = vec![session(
            "sb",
            "/p",
            "claude-sonnet-4-6",
            vec![
                req(0, 1, 0, 100, 0),
                req(300, 1, 10, 0, 0),
                req(601, 1, 10, 0, 0),
                req(4201, 1, 10, 0, 0),
                req(7802, 1, 10, 0, 0),
            ],
        )];
        let report = analyze_sessions(&sessions);
        let ttl = &report.ttl_counterfactual;
        assert_eq!(ttl.simulated_w5_write_events, 4);
        assert_eq!(ttl.simulated_w1_write_events, 2);
        assert_eq!(ttl.simulated_w5_write_tokens, 400);
        assert_eq!(ttl.simulated_w1_write_tokens, 200);
        // Intra gaps over 5m: 301, 3600, 3601 are > 300; the exact-300 gap
        // is not.
        assert_eq!(report.cadence.intra_session_gaps, 4);
        assert_eq!(report.cadence.intra_session_gaps_over_5m, 3);
    }

    // The refresh behavior itself: requests every 4 minutes for 20 minutes
    // span 1200s (well past one 5m TTL) yet never let the cache expire, so
    // the 5m strategy writes exactly once. Without read-refresh the
    // simulation would re-write on TTL expiry mid-stream.
    #[test]
    fn golden_reads_refresh_ttl() {
        let ts = [0u64, 240, 480, 720, 960, 1200];
        assert_eq!(simulated_write_events_reads_refresh_ttl(&ts, 300), 1);
        assert_eq!(simulated_write_events_reads_refresh_ttl(&ts, 3600), 1);
        // Degenerate inputs.
        assert_eq!(simulated_write_events_reads_refresh_ttl(&[], 300), 0);
        assert_eq!(simulated_write_events_reads_refresh_ttl(&[42], 300), 1);
        // Equal timestamps: zero gap, never an expiry.
        assert_eq!(simulated_write_events_reads_refresh_ttl(&[5, 5, 5], 300), 1);
    }

    // Write churn with multiple writes. Session A: writes 1000 (first),
    // then 300, then 200 -> total 1500, after-first 500, share 1/3.
    // Session B: single write 400 -> after-first 0 (not an offender).
    #[test]
    fn golden_write_churn_with_multiple_writes() {
        let sessions = vec![
            session(
                "churny",
                "/p",
                "claude-sonnet-4-6",
                vec![
                    req(0, 1, 0, 0, 1000),
                    req(60, 1, 50, 0, 300),
                    req(120, 1, 50, 200, 0),
                ],
            ),
            session(
                "clean",
                "/p",
                "claude-sonnet-4-6",
                vec![req(9000, 1, 0, 0, 400), req(9060, 1, 50, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        let churn = &report.write_churn;
        assert_eq!(churn.total_write_tokens, 1900);
        assert_eq!(churn.writes_after_first_tokens, 500);
        assert!(approx(churn.share, 500.0 / 1900.0));
        assert_eq!(churn.sessions_with_writes, 2);
        assert_eq!(churn.worst_sessions.len(), 1);
        let worst = &churn.worst_sessions[0];
        assert_eq!(worst.session_id, "churny");
        assert_eq!(worst.first_ts, 0);
        assert_eq!(worst.write_tokens, 1500);
        assert_eq!(worst.writes_after_first_tokens, 500);
        assert!(approx(worst.share, 500.0 / 1500.0));
    }

    // Equal-timestamp writes: the deterministic tuple order decides which
    // write is "first". Two writes at the same ts (300 and 1000 tokens at
    // identical input/read): the smaller write tuple sorts first
    // (cache_write_5m 300 < 1000), so after-first = 1000.
    #[test]
    fn churn_equal_ts_resolves_by_tuple_order() {
        let sessions = vec![session(
            "tie",
            "/p",
            "claude-sonnet-4-6",
            vec![req(100, 1, 0, 1000, 0), req(100, 1, 0, 300, 0)],
        )];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.write_churn.total_write_tokens, 1300);
        assert_eq!(report.write_churn.writes_after_first_tokens, 1000);
        // The prefix proxy is the first write in tuple order: 300.
        assert_eq!(report.ttl_counterfactual.simulated_w5_write_tokens, 300);
    }

    // A session with zero cache reads is counted by the cadence fact; if it
    // also has no writes it is excluded from the simulation.
    #[test]
    fn golden_zero_cache_read_session() {
        let sessions = vec![
            session(
                "no-cache-at-all",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 100, 0, 0, 0), req(60, 100, 0, 0, 0)],
            ),
            session(
                "healthy",
                "/p",
                "claude-sonnet-4-6",
                vec![req(900, 10, 0, 0, 500), req(960, 10, 600, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.cadence.sessions_zero_cache_read, 1);
        let ttl = &report.ttl_counterfactual;
        assert_eq!(ttl.sessions_simulated, 1);
        assert_eq!(ttl.sessions_without_writes_excluded, 1);
        assert_eq!(ttl.simulated_w5_write_events, 1);
        assert_eq!(ttl.simulated_w5_write_tokens, 500);
    }

    // Empty state: no sessions at all. Everything zero, no panic, no
    // advisory, and the scope line still ships.
    #[test]
    fn golden_empty_state() {
        let report = analyze_sessions(&[]);
        assert_eq!(report.sessions_analyzed, 0);
        assert_eq!(report.requests_analyzed, 0);
        assert!(approx(report.hit_rate.rate, 0.0));
        assert!(report.hit_rate.advisory.is_none());
        assert!(report.hit_rate.days_below_threshold.is_empty());
        assert!(approx(report.write_churn.share, 0.0));
        assert!(report.write_churn.worst_sessions.is_empty());
        let ttl = &report.ttl_counterfactual;
        assert_eq!(ttl.sessions_simulated, 0);
        assert_eq!(ttl.simulated_w5_write_events, 0);
        assert!(approx(ttl.usd_5m_strategy, 0.0));
        assert!(approx(ttl.priced_share_of_simulated_tokens, 0.0));
        assert!(ttl
            .verdict
            .contains("no sessions with observed cache writes"));
        assert!(approx(report.cadence.share_intra_gaps_over_5m, 0.0));
        assert!(approx(report.cadence.share_inter_gaps_under_1h, 0.0));
        assert_eq!(report.scope_line, SCOPE_LINE);
    }

    // Codex sessions never enter the analysis (no cache-write fields in
    // their records); only the Claude session is counted.
    #[test]
    fn codex_sessions_are_excluded() {
        let sessions = vec![
            codex_session("rollout-1", "/p"),
            session(
                "claude",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 10, 0, 0, 100)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.sessions_analyzed, 1);
        assert_eq!(report.requests_analyzed, 1);
        // The codex session's 100 cache-read tokens must NOT be in the rate.
        assert_eq!(report.hit_rate.cache_read_tokens, 0);
        assert_eq!(report.source, "claude_code");
    }

    // Cross-project interleaving: inter-session gaps are PER PROJECT, not
    // across the merged machine timeline.
    // Project A sessions: [0, 100] and [7000, 7100]: one gap of 6900 (not
    // under an hour). Project B sessions: [3000, 3100] and [3500, 3600]:
    // one gap of 400 (under an hour). Merged-timeline math would instead
    // see three gaps (100->3000, 3100->3500, 3600->7000) and a different
    // share; this test pins the per-project rule.
    #[test]
    fn golden_cross_project_interleaving() {
        let sessions = vec![
            session(
                "a1",
                "/a",
                "claude-sonnet-4-6",
                vec![req(0, 1, 0, 0, 10), req(100, 1, 5, 0, 0)],
            ),
            session(
                "b1",
                "/b",
                "claude-sonnet-4-6",
                vec![req(3000, 1, 0, 0, 10), req(3100, 1, 5, 0, 0)],
            ),
            session(
                "b2",
                "/b",
                "claude-sonnet-4-6",
                vec![req(3500, 1, 0, 0, 10), req(3600, 1, 5, 0, 0)],
            ),
            session(
                "a2",
                "/a",
                "claude-sonnet-4-6",
                vec![req(7000, 1, 0, 0, 10), req(7100, 1, 5, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.cadence.inter_session_gaps, 2);
        assert_eq!(report.cadence.inter_session_gaps_under_1h, 1);
        assert!(approx(report.cadence.share_inter_gaps_under_1h, 0.5));
        // Project scope: only /b's gap survives the filter.
        let scoped = analyze(&CacheInputs {
            scope_project: Some("/b"),
            sessions: &sessions,
            rate_fn: &sonnet_rates,
        });
        assert_eq!(scoped.sessions_analyzed, 2);
        assert_eq!(scoped.cadence.inter_session_gaps, 1);
        assert_eq!(scoped.cadence.inter_session_gaps_under_1h, 1);
    }

    // Out-of-order request tuples (as a hostile caller might supply them):
    // the analysis sorts internally, so the gap math matches the sorted
    // timeline exactly.
    #[test]
    fn golden_out_of_order_timestamps() {
        let unsorted = vec![
            req(5000, 10, 200, 0, 0),
            req(0, 10, 0, 0, 1000),
            req(700, 10, 200, 0, 0),
            req(100, 10, 200, 0, 0),
        ];
        let mut s = session("ooo", "/p", "claude-sonnet-4-6", Vec::new());
        s.requests = unsorted; // bypass the constructor's ordering
        let report = analyze_sessions(&[s]);
        let ttl = &report.ttl_counterfactual;
        // Identical to golden_single_session_simple_gaps: sorted gaps are
        // 100, 600, 4300 and the first write (t=0) is the proxy.
        assert_eq!(ttl.simulated_w5_write_events, 3);
        assert_eq!(ttl.simulated_w1_write_events, 2);
        assert_eq!(ttl.simulated_w5_write_tokens, 3000);
        assert_eq!(ttl.simulated_w1_write_tokens, 2000);
    }

    // Overlapping sessions in one project: the inter-session gap saturates
    // to zero and counts as under an hour (no panic, no underflow).
    #[test]
    fn overlapping_sessions_gap_saturates_to_zero() {
        let sessions = vec![
            session(
                "s1",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 1, 0, 0, 10), req(10_000, 1, 5, 0, 0)],
            ),
            session(
                "s2",
                "/p",
                "claude-sonnet-4-6",
                vec![req(500, 1, 0, 0, 10), req(600, 1, 5, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.cadence.inter_session_gaps, 1);
        assert_eq!(report.cadence.inter_session_gaps_under_1h, 1);
    }

    // Per-day broken-cache advisory: day one is healthy (rate 0.9), day two
    // broken (rate 0.0); the advisory names exactly the broken day even
    // though the overall rate stays above the threshold.
    #[test]
    fn broken_day_advisory_fires_per_day() {
        const DAY: u64 = 86_400;
        let sessions = vec![
            session(
                "day1",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 100, 0, 0, 900), req(60, 100, 8900, 0, 0)],
            ),
            session(
                "day2",
                "/p",
                "claude-sonnet-4-6",
                vec![req(DAY, 100, 0, 0, 0), req(DAY + 60, 100, 0, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        let hr = &report.hit_rate;
        // Overall: read 8900 over (200 + 8900 + 900) + 200 = 10200 -> 0.873.
        assert_eq!(hr.input_side_tokens, 10_200);
        assert!(hr.rate > HIT_RATE_BROKEN_THRESHOLD);
        assert_eq!(hr.days_below_threshold.len(), 1);
        assert_eq!(hr.days_below_threshold[0].day, "1970-01-02");
        assert!(approx(hr.days_below_threshold[0].rate, 0.0));
        assert!(hr.advisory.is_some());
    }

    // Unpriced dominant models are excluded from dollars but stay visible:
    // share and the model list disclose the gap.
    #[test]
    fn unpriced_models_excluded_from_dollars_but_disclosed() {
        let sessions = vec![
            session(
                "priced",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 10, 0, 0, 1000)],
            ),
            session(
                "unpriced",
                "/p",
                "claude-fable-5",
                vec![req(9000, 10, 0, 0, 3000)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        let ttl = &report.ttl_counterfactual;
        // Each session: 1 simulated event per strategy, tokens = proxy.
        assert_eq!(ttl.simulated_w1_write_tokens, 4000);
        // Only the sonnet session is priced: 1000 * 5.7 / 1e6.
        assert!(approx(ttl.usd_1h_strategy, 1000.0 * 5.7 / 1e6));
        assert_eq!(ttl.unpriced_models, vec!["claude-fable-5".to_string()]);
        // priced share = 2000 / 8000 (both strategies' tokens summed).
        assert!(approx(
            ttl.priced_share_of_simulated_tokens,
            2000.0 / 8000.0
        ));
    }

    // Nothing priced at all: dollars are zero and the share is zero, but
    // the token-level verdict still resolves.
    #[test]
    fn all_unpriced_still_yields_token_verdict() {
        let sessions = vec![session(
            "s",
            "/p",
            "claude-fable-5",
            vec![req(0, 10, 0, 0, 1000), req(5000, 10, 100, 0, 0)],
        )];
        let report = analyze(&CacheInputs {
            scope_project: None,
            sessions: &sessions,
            rate_fn: &no_rates,
        });
        let ttl = &report.ttl_counterfactual;
        assert!(approx(ttl.usd_5m_strategy, 0.0));
        assert!(approx(ttl.usd_1h_strategy, 0.0));
        assert!(approx(ttl.priced_share_of_simulated_tokens, 0.0));
        // W5 = 2 events (gap 5000 > 300), W1 = 2 events (5000 > 3600).
        // weighted: 1.15 x 2000 = 2300 vs 1.9 x 2000 = 3800: 5m cheaper.
        assert!(ttl.verdict.contains("5m TTL strategy is cheaper"));
    }

    // The I2 resume-copy scenario feeding retention: this is covered end to
    // end in usage::claude_code (reader) and here at the analysis level: the
    // same request must never count twice even if a caller hands us two
    // sessions that TOGETHER carry each deduped record exactly once.
    #[test]
    fn resume_split_sessions_count_each_request_once() {
        // After global dedup the resumed session splits as: original keeps
        // r1 (the write), resume keeps r2 (a read 30 minutes later).
        let sessions = vec![
            session(
                "a-original",
                "/p",
                "claude-sonnet-4-6",
                vec![req(0, 10, 0, 0, 1000)],
            ),
            session(
                "b-resume",
                "/p",
                "claude-sonnet-4-6",
                vec![req(1800, 10, 500, 0, 0)],
            ),
        ];
        let report = analyze_sessions(&sessions);
        assert_eq!(report.requests_analyzed, 2);
        assert_eq!(report.ttl_counterfactual.observed_write_tokens_1h, 1000);
        assert_eq!(report.hit_rate.cache_read_tokens, 500);
        // The split is inherited from the I2 winner rule: the 1800s gap is
        // an inter-session gap (under an hour), not an intra-session one.
        assert_eq!(report.cadence.intra_session_gaps, 0);
        assert_eq!(report.cadence.inter_session_gaps, 1);
        assert_eq!(report.cadence.inter_session_gaps_under_1h, 1);
    }

    // Determinism: caller order must not change a single field.
    #[test]
    fn session_order_does_not_change_report() {
        let a = session(
            "a",
            "/p",
            "claude-sonnet-4-6",
            vec![req(0, 10, 0, 0, 100), req(400, 10, 50, 0, 30)],
        );
        let b = session(
            "b",
            "/q",
            "claude-sonnet-4-6",
            vec![req(50, 10, 0, 200, 0), req(60, 10, 5, 0, 0)],
        );
        let c = codex_session("r", "/p");
        let fwd = analyze_sessions(&[a.clone(), b.clone(), c.clone()]);
        let rev = analyze_sessions(&[c, b, a]);
        let fwd_json = serde_json::to_string(&fwd).unwrap();
        let rev_json = serde_json::to_string(&rev).unwrap();
        assert_eq!(fwd_json, rev_json);
    }

    // The pinned marginal multipliers must equal the pricing table's
    // derivation for every Anthropic model, or the break-even statement
    // (and its rustdoc) is stale.
    #[test]
    fn marginal_multipliers_match_pricing_table_for_anthropic() {
        use tolkin_core::pricing;
        let mut checked = 0;
        for m in pricing::all() {
            if !matches!(m.provider, tolkin_core::Provider::Anthropic) {
                continue;
            }
            let read = m.cache_read.expect("anthropic publishes cache_read");
            let w5 = m.cache_write_5m.expect("anthropic publishes 5m writes");
            let w1 = m.cache_write_1h.expect("anthropic publishes 1h writes");
            assert!(
                approx((w5 - read) / m.input, MARGINAL_MULTIPLIER_5M),
                "{}: 5m marginal drifted from 1.15x",
                m.id
            );
            assert!(
                approx((w1 - read) / m.input, MARGINAL_MULTIPLIER_1H),
                "{}: 1h marginal drifted from 1.9x",
                m.id
            );
            checked += 1;
        }
        assert!(checked >= 4, "expected the four Anthropic models");
    }

    // Labels are pinned to the tiers module vocabulary verbatim.
    #[test]
    fn labels_match_tiers_vocabulary() {
        assert_eq!(LABEL_MEASURED, crate::tiers::LABEL_MEASURED);
        assert_eq!(LABEL_IDENTIFIED, crate::tiers::LABEL_IDENTIFIED);
    }
}
