//! Three measured advisories: model mix, output share, and cap runway.
//!
//! All three are computed exclusively from the Measured tier (Tier 3 ground
//! truth): actual spend aggregated from local session logs. Every advisory
//! is surfaced in `tolkin stats` (plain + --json), the TUI Spend tab, and
//! `tolkin report --html`, computed here once so the surfaces cannot disagree.
//!
//! # Model mix advisory
//!
//! Identifies the highest-cost model (most dollars of priced spend) and the
//! cheapest-tier model (lowest input price among priced models). Two
//! advisories, each rendered only when its threshold condition holds:
//!
//! - When the most expensive model carries over 25 % of priced spend: note
//!   that routing-sensitive work may be over-concentrated on the frontier
//!   model. This is a community heuristic, cited as such.
//! - When the cheapest-tier model carries under 10 % of priced spend: note
//!   possible under-use of cheap fan-out. Same heuristic basis.
//!
//! Both thresholds come from community-cited research (the 25 % / 10 %
//! pair appears in independent tokopt blueprints and Claude-workload
//! analysis discussions; they are heuristics and the notes say so).
//! Unpriced models are excluded from the shares and disclosed, matching the
//! existing Measured tier convention.
//!
//! # Output share advisory
//!
//! Output tokens and output dollars as a share of the priced session bill,
//! with exactly one framing sentence: this share is the most an output-side
//! tool could possibly touch. This ends the output-compression denominator
//! argument with the user's own number (from their own logs, measured tier).
//!
//! # Cap runway advisory
//!
//! When a `monthly_cap_usd` is set in config.toml and measured data exists:
//!
//! - Month-to-date priced spend (calendar month in UTC, consistent with the
//!   existing UTC display convention).
//! - Projection from the trailing 7-day average daily burn: the date the cap
//!   is reached at the current rate.
//! - Same projection from the trailing 30-day average.
//!
//! Formula (anchored to the calendar month in UTC):
//!
//! ```text
//! mtd_spend = sum of priced-model cost over sessions whose last_ts falls in
//!             the current calendar month (UTC day "YYYY-MM" prefix match)
//!
//! daily_rate_Nd = sum of priced-model cost over sessions whose last_ts
//!                  falls within the last N calendar days (inclusive of today)
//!                  divided by N
//!
//! remaining = monthly_cap_usd - mtd_spend
//! days_to_cap = remaining / daily_rate_Nd   (if daily_rate_Nd > 0)
//!
//! cap_date = today's UTC date + ceil(days_to_cap) days
//! ```
//!
//! Projection requires at least 3 active days (days with nonzero priced spend)
//! within the window. Fewer active days renders "fewer than 3 active days in
//! window; projection requires at least 3". When the cap has already been
//! exceeded, renders "cap already reached this month". When the cap is unset,
//! nothing renders (no advisory, no nagging).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::tiers::Measured;
use crate::usage::types::SessionUsage;

/// Minimum active days in the trailing window before a projection is shown.
pub const MIN_ACTIVE_DAYS_FOR_PROJECTION: u64 = 3;

/// Frontier-model share threshold for the over-concentration advisory.
/// Source: community heuristic cited in independent tokopt blueprints and
/// Claude workload analysis discussions (2025-2026). Cited as heuristic.
pub const FRONTIER_SHARE_THRESHOLD: f64 = 0.25;

/// Cheap-model share threshold for the under-use advisory.
/// Same source as FRONTIER_SHARE_THRESHOLD. Cited as heuristic.
pub const CHEAP_SHARE_THRESHOLD: f64 = 0.10;

/// Inputs for [`compute`]. The module never reads the environment or clock.
pub struct AdvisoryInputs<'a> {
    /// The Measured tier, already computed by `tiers::compute`.
    pub measured: &'a Measured,
    /// All ingested sessions (same slice the measured tier was built from).
    pub sessions: &'a [SessionUsage],
    /// Pricing lookup, called with the RAW model id.
    pub cost_fn: &'a dyn Fn(&str, &crate::usage::types::UsageTotals) -> Option<f64>,
    /// Pricing lookup for a single input token count (to find cheapest model
    /// by its input rate). Called with model id; returns Some(input_rate) or None.
    pub input_rate_fn: &'a dyn Fn(&str) -> Option<f64>,
    /// Optional monthly cap from config.toml.
    pub monthly_cap_usd: Option<f64>,
    /// Current UTC timestamp (epoch seconds). Injected for reproducibility.
    pub now: u64,
}

/// The full advisory block, attached to the measured tier.
#[derive(Debug, Serialize)]
pub struct AdvisoryBlock {
    pub label: &'static str,
    pub model_mix: ModelMix,
    pub output_share: OutputShare,
    /// Present only when monthly_cap_usd is configured and measured data exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_runway: Option<CapRunway>,
}

/// Model-mix shares and conditional advisories.
#[derive(Debug, Serialize)]
pub struct ModelMix {
    pub label: &'static str,
    pub priced_spend_total: f64,
    pub unpriced_models_excluded: Vec<String>,
    /// The top model (highest priced spend) and its share.
    pub frontier: Option<ModelShare>,
    /// The cheapest-tier model (lowest input rate) and its share.
    pub cheap: Option<ModelShare>,
    /// Over-concentration advisory. Present when frontier share > 25%.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontier_advisory: Option<String>,
    /// Under-use advisory. Present when cheap share < 10%.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cheap_advisory: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelShare {
    pub model: String,
    pub spend_usd: f64,
    pub share: f64,
}

/// Output token and dollar share of the priced session bill.
#[derive(Debug, Serialize)]
pub struct OutputShare {
    pub label: &'static str,
    pub output_tokens: u64,
    pub output_cost_usd: f64,
    pub priced_bill_usd: f64,
    /// output_cost_usd / priced_bill_usd, 0.0 when bill is 0.
    pub share: f64,
    /// The framing sentence.
    pub framing: String,
}

/// Cap runway projections from trailing 7-day and 30-day rates.
#[derive(Debug, Serialize)]
pub struct CapRunway {
    pub label: &'static str,
    pub monthly_cap_usd: f64,
    pub mtd_spend_usd: f64,
    pub remaining_usd: f64,
    /// Trailing 7-day average daily burn.
    pub rate_7d: DailyRate,
    /// Trailing 30-day average daily burn.
    pub rate_30d: DailyRate,
    /// True when mtd_spend >= monthly_cap_usd.
    pub cap_already_reached: bool,
}

#[derive(Debug, Serialize)]
pub struct DailyRate {
    pub window_days: u64,
    pub active_days: u64,
    pub avg_daily_usd: f64,
    /// None when active_days < MIN_ACTIVE_DAYS_FOR_PROJECTION.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection_note: Option<String>,
    /// None when projection is not available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_cap_date: Option<String>,
}

/// Compute the advisory block from the measured tier.
pub fn compute(inputs: &AdvisoryInputs) -> AdvisoryBlock {
    let model_mix = compute_model_mix(inputs);
    let output_share = compute_output_share(inputs);
    let cap_runway = compute_cap_runway(inputs);
    AdvisoryBlock {
        label: "measured advisories (ground truth basis)",
        model_mix,
        output_share,
        cap_runway,
    }
}

fn compute_model_mix(inputs: &AdvisoryInputs) -> ModelMix {
    let measured = inputs.measured;
    // Collect priced and unpriced model costs.
    let mut priced: Vec<(String, f64)> = Vec::new();
    let mut unpriced: Vec<String> = Vec::new();
    for (model, mu) in &measured.by_model {
        match mu.cost_usd {
            Some(c) if c > 0.0 => priced.push((model.clone(), c)),
            Some(_) => {
                // Priced but zero cost: include in the priced set so the model
                // appears in the by-model accounting. Zero-cost models do not
                // affect shares meaningfully but staying in the set keeps the
                // math consistent.
                priced.push((model.clone(), 0.0));
            }
            None => unpriced.push(model.clone()),
        }
    }
    // Also collect any model in unpriced_models that has zero cost_usd (None).
    // (measured.unpriced_models is already the canonical list of unpriceable models.)
    for u in &measured.unpriced_models {
        if !unpriced.contains(&u.model) {
            unpriced.push(u.model.clone());
        }
    }
    unpriced.sort();

    let total: f64 = priced.iter().map(|(_, c)| c).sum();

    if priced.is_empty() || total == 0.0 {
        return ModelMix {
            label: "ground truth",
            priced_spend_total: 0.0,
            unpriced_models_excluded: unpriced,
            frontier: None,
            cheap: None,
            frontier_advisory: None,
            cheap_advisory: None,
        };
    }

    // Frontier: highest spend model.
    let frontier_entry = priced
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("non-empty");
    let frontier_share = frontier_entry.1 / total;

    // Cheap-tier: model with the lowest input rate among priced models.
    let mut cheapest_model: Option<String> = None;
    let mut cheapest_rate = f64::INFINITY;
    for (model, _) in &priced {
        if let Some(rate) = (inputs.input_rate_fn)(model) {
            if rate < cheapest_rate {
                cheapest_rate = rate;
                cheapest_model = Some(model.clone());
            }
        }
    }

    let cheap_share_val: f64 = cheapest_model
        .as_ref()
        .and_then(|m| priced.iter().find(|(n, _)| n == m))
        .map(|(_, c)| c / total)
        .unwrap_or(0.0);

    let cheap_spend: f64 = cheapest_model
        .as_ref()
        .and_then(|m| priced.iter().find(|(n, _)| n == m))
        .map(|(_, c)| *c)
        .unwrap_or(0.0);

    let frontier_advisory = if frontier_share > FRONTIER_SHARE_THRESHOLD {
        Some(format!(
            "advisory (community heuristic): the frontier model {} carries {:.1}% of priced spend. \
             When over 25% of spend concentrates on the highest-cost model, routing-sensitive work \
             may be over-concentrated on the frontier; consider whether cheaper models can handle \
             a share of this workload. The 25% threshold is a community heuristic, not a hard rule.",
            frontier_entry.0,
            frontier_share * 100.0
        ))
    } else {
        None
    };

    let cheap_advisory = cheapest_model.as_ref().and_then(|m| {
        if cheap_share_val < CHEAP_SHARE_THRESHOLD {
            Some(format!(
                "advisory (community heuristic): the cheapest-tier model {} carries {:.1}% of \
                 priced spend. When the small model is under 10% of spend, cheap fan-out \
                 (parallel sub-tasks routed to a lower-cost model) may be under-used. \
                 The 10% threshold is a community heuristic, not a hard rule.",
                m,
                cheap_share_val * 100.0
            ))
        } else {
            None
        }
    });

    let frontier_model = Some(ModelShare {
        model: frontier_entry.0.clone(),
        spend_usd: frontier_entry.1,
        share: frontier_share,
    });

    let cheap_model = cheapest_model.map(|m| ModelShare {
        model: m,
        spend_usd: cheap_spend,
        share: cheap_share_val,
    });

    ModelMix {
        label: "ground truth",
        priced_spend_total: total,
        unpriced_models_excluded: unpriced,
        frontier: frontier_model,
        cheap: cheap_model,
        frontier_advisory,
        cheap_advisory,
    }
}

fn compute_output_share(inputs: &AdvisoryInputs) -> OutputShare {
    let measured = inputs.measured;
    let output_tokens = measured.totals.output_tokens;
    let priced_bill = measured.cost_usd_total;

    // Compute output cost: sum across priced models of (output_tokens * output_rate).
    // We use the cost_fn to get total cost, then subtract input-side cost.
    // More precisely: output_cost = cost_fn(model, output-only totals).
    // Since cost_fn takes full UsageTotals and returns total cost, we approximate
    // by computing output_fraction = output_tokens / (input_side + output_tokens)
    // then scaling. But the spec says "output dollars as a share of the priced bill",
    // so we need to compute output-only cost.
    //
    // The cleanest approach: for each priced model, compute cost with output only
    // and cost with input side only, compare to total. But cost_fn is opaque.
    // Instead we directly compute output cost by looking at the pricing table.
    // We use cost::output_cost_usd which is available via the cost module.
    let output_cost_usd: f64 = measured
        .by_model
        .iter()
        .filter_map(|(model, mu)| {
            // Only include priced models.
            mu.cost_usd?;
            let output_only_totals = crate::usage::types::UsageTotals {
                input_tokens: 0,
                output_tokens: mu.totals.output_tokens,
                cache_read_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
            };
            (inputs.cost_fn)(model, &output_only_totals)
        })
        .sum();

    let share = if priced_bill > 0.0 {
        output_cost_usd / priced_bill
    } else {
        0.0
    };

    let framing = format!(
        "Output tokens are {:.1}% of the priced bill ({} tokens, ${:.4} of ${:.4} total). \
         This is the most any output-side tool could possibly save from this workload.",
        share * 100.0,
        format_commas(output_tokens),
        output_cost_usd,
        priced_bill
    );

    OutputShare {
        label: "ground truth",
        output_tokens,
        output_cost_usd,
        priced_bill_usd: priced_bill,
        share,
        framing,
    }
}

fn compute_cap_runway(inputs: &AdvisoryInputs) -> Option<CapRunway> {
    let cap = inputs.monthly_cap_usd?;

    // Build per-day cost map from sessions.
    // day key "YYYY-MM-DD" -> cost_usd (priced models only).
    let mut by_day: BTreeMap<String, f64> = BTreeMap::new();
    for session in inputs.sessions {
        for (model, mu) in &session.by_model {
            if let Some(cost) = (inputs.cost_fn)(model, mu) {
                // Use the session's by_day split to attribute cost per day.
                // If by_day is empty, attribute all cost to last_ts day.
                if session.by_day.is_empty() {
                    let day = utc_day_from_secs(session.last_ts);
                    *by_day.entry(day).or_insert(0.0) += cost;
                } else {
                    // Attribute cost proportionally to each day's token share.
                    let total_input_side: u64 =
                        session.totals.input_side() + session.totals.output_tokens;
                    for (day_key, day_totals) in &session.by_day {
                        let day_vol = day_totals.input_side() + day_totals.output_tokens;
                        if total_input_side > 0 && day_vol > 0 {
                            let frac = day_vol as f64 / total_input_side as f64;
                            *by_day.entry(day_key.clone()).or_insert(0.0) += cost * frac;
                        } else if total_input_side == 0 {
                            // All zero: attribute to last_ts day.
                            let day = utc_day_from_secs(session.last_ts);
                            *by_day.entry(day).or_insert(0.0) += cost;
                        }
                    }
                }
            }
        }
    }

    let today_str = utc_day_from_secs(inputs.now);
    let current_month = &today_str[..7]; // "YYYY-MM"

    // Month-to-date spend.
    let mtd_spend: f64 = by_day
        .iter()
        .filter(|(day, _)| day.starts_with(current_month))
        .map(|(_, cost)| cost)
        .sum();

    let remaining = cap - mtd_spend;
    let cap_already_reached = mtd_spend >= cap;

    let rate_7d = trailing_daily_rate(&by_day, &today_str, 7, remaining, cap_already_reached);
    let rate_30d = trailing_daily_rate(&by_day, &today_str, 30, remaining, cap_already_reached);

    Some(CapRunway {
        label: "ground truth",
        monthly_cap_usd: cap,
        mtd_spend_usd: mtd_spend,
        remaining_usd: remaining.max(0.0),
        rate_7d,
        rate_30d,
        cap_already_reached,
    })
}

fn trailing_daily_rate(
    by_day: &BTreeMap<String, f64>,
    today: &str,
    window_days: u64,
    remaining: f64,
    cap_already_reached: bool,
) -> DailyRate {
    // Collect the window: today and the previous (window_days - 1) days.
    let today_secs = day_str_to_secs(today);
    let window_start_secs = today_secs.saturating_sub((window_days - 1) * 86_400);

    let mut total_cost = 0.0f64;
    let mut active_days: u64 = 0;

    for (day, cost) in by_day {
        let day_secs = day_str_to_secs(day);
        if day_secs >= window_start_secs && day_secs <= today_secs {
            total_cost += cost;
            if *cost > 0.0 {
                active_days += 1;
            }
        }
    }

    let avg_daily_usd = total_cost / window_days as f64;

    if cap_already_reached {
        return DailyRate {
            window_days,
            active_days,
            avg_daily_usd,
            projection_note: Some("cap already reached this month".to_string()),
            projected_cap_date: None,
        };
    }

    if active_days < MIN_ACTIVE_DAYS_FOR_PROJECTION {
        return DailyRate {
            window_days,
            active_days,
            avg_daily_usd,
            projection_note: Some(format!(
                "fewer than {} active days in the {}-day window; projection requires at least {}",
                MIN_ACTIVE_DAYS_FOR_PROJECTION, window_days, MIN_ACTIVE_DAYS_FOR_PROJECTION
            )),
            projected_cap_date: None,
        };
    }

    if avg_daily_usd <= 0.0 {
        return DailyRate {
            window_days,
            active_days,
            avg_daily_usd,
            projection_note: Some("daily rate is zero; no projection available".to_string()),
            projected_cap_date: None,
        };
    }

    // days_to_cap = remaining / avg_daily_usd, rounded up to the next whole day.
    let days_to_cap = (remaining / avg_daily_usd).ceil() as i64;
    if days_to_cap <= 0 {
        return DailyRate {
            window_days,
            active_days,
            avg_daily_usd,
            projection_note: Some("cap already reached this month".to_string()),
            projected_cap_date: None,
        };
    }

    let today_secs_u64 = day_str_to_secs(today);
    let cap_secs = today_secs_u64 + (days_to_cap as u64) * 86_400;
    let cap_date = utc_day_from_secs(cap_secs);

    DailyRate {
        window_days,
        active_days,
        avg_daily_usd,
        projection_note: None,
        projected_cap_date: Some(cap_date),
    }
}

/// "YYYY-MM-DD" from epoch seconds.
pub fn utc_day_from_secs(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{mo:02}-{d:02}")
}

/// Parse "YYYY-MM-DD" back to epoch seconds (midnight UTC). Returns 0 on parse failure.
fn day_str_to_secs(day: &str) -> u64 {
    // Expected format: "YYYY-MM-DD" (10 chars)
    if day.len() != 10 {
        return 0;
    }
    let year: i64 = day[..4].parse().unwrap_or(0);
    let month: u32 = day[5..7].parse().unwrap_or(0);
    let day_num: u32 = day[8..10].parse().unwrap_or(0);
    days_from_civil(year, month, day_num) * 86_400
}

/// Howard Hinnant's days_from_civil: calendar (year, month, day) to days
/// since Unix epoch. Inverse of civil_from_days.
fn days_from_civil(y: i64, m: u32, d: u32) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch = era * 146_097 + doe as i64 - 719_468;
    days_since_epoch.max(0) as u64
}

/// Howard Hinnant's civil_from_days: days since epoch to (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(*b));
    }
    out
}

/// Two-line compact representation for the TUI Spend tab.
/// Returns at most two compact lines covering the highest-value advisories:
/// output share (always) and cap runway (when configured).
pub fn tui_compact_lines(block: &AdvisoryBlock) -> Vec<String> {
    let mut lines = Vec::with_capacity(2);

    // Output share line (always shown when priced bill > 0).
    let os = &block.output_share;
    if os.priced_bill_usd > 0.0 {
        lines.push(format!(
            "output share  {:.1}% of bill ({} tokens, ${:.4})",
            os.share * 100.0,
            format_commas(os.output_tokens),
            os.output_cost_usd,
        ));
    }

    // Cap runway line (only when configured and projection available).
    if let Some(ref cap) = block.cap_runway {
        if cap.cap_already_reached {
            lines.push(format!(
                "cap runway    cap ${:.2} already reached this month (MTD ${:.2})",
                cap.monthly_cap_usd, cap.mtd_spend_usd
            ));
        } else {
            // Prefer the 7-day projection if available; fall back to 30-day.
            let proj_line = cap
                .rate_7d
                .projected_cap_date
                .as_ref()
                .map(|date| {
                    format!(
                        "cap runway    at your 7-day rate you reach ${:.2} on {} (MTD ${:.2})",
                        cap.monthly_cap_usd, date, cap.mtd_spend_usd
                    )
                })
                .or_else(|| {
                    cap.rate_30d.projected_cap_date.as_ref().map(|date| {
                        format!(
                            "cap runway    at your 30-day rate you reach ${:.2} on {} (MTD ${:.2})",
                            cap.monthly_cap_usd, date, cap.mtd_spend_usd
                        )
                    })
                });
            if let Some(line) = proj_line {
                lines.push(line);
            }
        }
    }

    lines.truncate(2);
    lines
}

/// One advisory's detail content for the TUI modal: a short title plus the
/// full advisory paragraphs (the framing sentence and the lever sentences
/// reused verbatim from the existing advisory copy).
#[derive(Clone, Debug)]
pub struct AdvisorySection {
    pub title: String,
    pub paragraphs: Vec<String>,
}

/// Detail sections for the TUI advisory modal, index-aligned with
/// [`tui_compact_lines`]: section i is the detail for compact line i. The
/// inclusion conditions are mirrored exactly so the selectable list and the
/// detail view can never disagree.
pub fn tui_detail_sections(block: &AdvisoryBlock) -> Vec<AdvisorySection> {
    let mut out = Vec::with_capacity(2);

    let os = &block.output_share;
    if os.priced_bill_usd > 0.0 {
        let mut paragraphs = vec![os.framing.clone()];
        // The model-mix lever sentences ride along with the output-share
        // detail: they are the existing advisory copy naming what to pull.
        if let Some(ref adv) = block.model_mix.frontier_advisory {
            paragraphs.push(adv.clone());
        }
        if let Some(ref adv) = block.model_mix.cheap_advisory {
            paragraphs.push(adv.clone());
        }
        out.push(AdvisorySection {
            title: "output share".to_string(),
            paragraphs,
        });
    }

    if let Some(ref cap) = block.cap_runway {
        let mut paragraphs = Vec::new();
        if cap.cap_already_reached {
            paragraphs.push(format!(
                "cap ${:.2} already reached this month (MTD ${:.2}).",
                cap.monthly_cap_usd, cap.mtd_spend_usd
            ));
        } else {
            paragraphs.push(format!(
                "MTD spend ${:.2}, remaining ${:.2} of cap ${:.2}.",
                cap.mtd_spend_usd, cap.remaining_usd, cap.monthly_cap_usd
            ));
            match (
                &cap.rate_7d.projected_cap_date,
                &cap.rate_7d.projection_note,
            ) {
                (Some(date), _) => paragraphs.push(format!(
                    "at your 7-day rate (${:.2}/day) you reach ${:.2} on {}.",
                    cap.rate_7d.avg_daily_usd, cap.monthly_cap_usd, date
                )),
                (None, Some(note)) => paragraphs.push(format!("7-day projection: {note}.")),
                (None, None) => {}
            }
            match (
                &cap.rate_30d.projected_cap_date,
                &cap.rate_30d.projection_note,
            ) {
                (Some(date), _) => paragraphs.push(format!(
                    "at your 30-day rate (${:.2}/day) you reach ${:.2} on {}.",
                    cap.rate_30d.avg_daily_usd, cap.monthly_cap_usd, date
                )),
                (None, Some(note)) => paragraphs.push(format!("30-day projection: {note}.")),
                (None, None) => {}
            }
        }
        // Mirror the compact-line condition: the line renders only when the
        // cap is reached or at least one projection is available.
        let included = cap.cap_already_reached
            || cap.rate_7d.projected_cap_date.is_some()
            || cap.rate_30d.projected_cap_date.is_some();
        if included {
            out.push(AdvisorySection {
                title: "cap runway".to_string(),
                paragraphs,
            });
        }
    }

    out.truncate(2);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiers::{Measured, ModelUsage, UnpricedModel};
    use crate::usage::types::{SessionUsage, UsageSource, UsageTotals};
    use std::collections::BTreeMap;

    fn usage(input: u64, output: u64, read: u64, w5: u64, w1: u64) -> UsageTotals {
        UsageTotals {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: read,
            cache_write_5m_tokens: w5,
            cache_write_1h_tokens: w1,
        }
    }

    fn measured_two_models(m1: &str, c1: f64, m2: &str, c2: f64, total_out: u64) -> Measured {
        let mut by_model = BTreeMap::new();
        by_model.insert(
            m1.to_string(),
            ModelUsage {
                totals: usage(1000, total_out / 2, 0, 0, 0),
                cost_usd: Some(c1),
            },
        );
        by_model.insert(
            m2.to_string(),
            ModelUsage {
                totals: usage(100, total_out / 2, 0, 0, 0),
                cost_usd: Some(c2),
            },
        );
        let total_cost = c1 + c2;
        Measured {
            label: "ground truth",
            sessions: 2,
            first_ts: Some(1000),
            last_ts: Some(2000),
            totals: usage(1100, total_out, 0, 0, 0),
            by_model,
            cost_usd_total: total_cost,
            unpriced_models: vec![],
            cache_hit_rate: 0.0,
        }
    }

    fn no_cost(_model: &str, _t: &UsageTotals) -> Option<f64> {
        None
    }

    fn no_rate(_model: &str) -> Option<f64> {
        None
    }

    // Helper: input_rate_fn that assigns 3.0 to "claude-opus-4" and 0.25 to
    // "claude-haiku-3-5" (a frontier/cheap pair).
    fn duo_rates(model: &str) -> Option<f64> {
        match model {
            "claude-opus-4" => Some(15.0),
            "claude-haiku-3-5" => Some(0.80),
            _ => None,
        }
    }

    // cost_fn: opus costs are passed through; haiku costs are passed through.
    fn duo_cost(model: &str, t: &UsageTotals) -> Option<f64> {
        let rate = match model {
            "claude-opus-4" => 15.0,
            "claude-haiku-3-5" => 0.80,
            _ => return None,
        };
        Some(
            t.input_tokens as f64 * rate / 1_000_000.0
                + t.output_tokens as f64 * rate * 5.0 / 1_000_000.0,
        )
    }

    /// Golden: exactly 25% frontier share should NOT fire advisory (threshold
    /// is strictly over 25%). The frontier is the highest-spend model.
    #[test]
    fn model_mix_frontier_exactly_25pct_no_advisory() {
        // frontier (highest-spend model) must be exactly 25.0%.
        // Use a measured with 3 models: A=$3.00, B=$3.00, C=$3.00, D=$1.00.
        // Wait - the simpler approach: define "frontier" as highest-spend model.
        // With one model at $1.00 and three at $1.00 each (4 equal), frontier
        // ties but the BTreeMap max gives a deterministic winner.
        // Easier: just use a raw Measured where one model has exactly 25% spend.
        let mut by_model = std::collections::BTreeMap::new();
        // frontier model = "zz-expensive" at $1.00 out of $4.00 total = 25%.
        // (BTreeMap iteration is alphabetical; "aa-cheap" < "zz-expensive",
        // so max_by picks "zz-expensive" because $1.00 = $1.00 ties, but
        // we need strict highest. Make "zz-expensive" the ONLY model at
        // exactly 25% by having 4 equal-spend models and the tie-breaker.)
        //
        // Cleanest: two models where the FIRST (alphabetically higher under
        // max_by semantics) carries exactly 25%. This is impossible with 2
        // models since the frontier is always >= 50%. Use direct construction.
        //
        // To have frontier = 25%, need total = 4 * frontier_spend.
        // Three other models at equal spend:
        // frontier = $1.00, others combined = $3.00. frontier_share = 25%.
        by_model.insert(
            "aa-cheap".to_string(),
            crate::tiers::ModelUsage {
                totals: usage(100, 50, 0, 0, 0),
                cost_usd: Some(1.0),
            },
        );
        by_model.insert(
            "bb-cheap".to_string(),
            crate::tiers::ModelUsage {
                totals: usage(100, 50, 0, 0, 0),
                cost_usd: Some(1.0),
            },
        );
        by_model.insert(
            "cc-cheap".to_string(),
            crate::tiers::ModelUsage {
                totals: usage(100, 50, 0, 0, 0),
                cost_usd: Some(1.0),
            },
        );
        by_model.insert(
            "zz-expensive".to_string(),
            crate::tiers::ModelUsage {
                totals: usage(100, 50, 0, 0, 0),
                cost_usd: Some(1.0),
            },
        );
        let measured = crate::tiers::Measured {
            label: "ground truth",
            sessions: 4,
            first_ts: Some(1000),
            last_ts: Some(2000),
            totals: usage(400, 200, 0, 0, 0),
            by_model,
            cost_usd_total: 4.0,
            unpriced_models: vec![],
            cache_hit_rate: 0.0,
        };
        fn four_rates(model: &str) -> Option<f64> {
            match model {
                "zz-expensive" => Some(15.0), // highest input rate = frontier candidate
                "aa-cheap" | "bb-cheap" | "cc-cheap" => Some(0.80),
                _ => None,
            }
        }
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &no_cost,
            input_rate_fn: &four_rates,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        // Every model has $1.00 spend; frontier is whichever max_by picks (ties
        // go to the last seen, which in BTreeMap ascending order is "zz-expensive").
        let frontier = block.model_mix.frontier.as_ref().expect("frontier present");
        // frontier share = 1.0 / 4.0 = 0.25 exactly. Threshold is > 0.25 (strict).
        assert!(
            (frontier.share - 0.25).abs() < 1e-9,
            "frontier share must be exactly 25%: {}",
            frontier.share
        );
        assert!(
            block.model_mix.frontier_advisory.is_none(),
            "25% exactly must not fire: {:?}",
            block.model_mix.frontier_advisory
        );
    }

    /// Golden: over 25% fires advisory.
    #[test]
    fn model_mix_frontier_over_25pct_fires_advisory() {
        // opus = 1.01 / (1.01 + 3.0) = 25.2%
        let measured = measured_two_models("claude-opus-4", 1.01, "claude-haiku-3-5", 3.0, 100);
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &duo_cost,
            input_rate_fn: &duo_rates,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        assert!(
            block.model_mix.frontier_advisory.is_some(),
            "over 25% must fire advisory"
        );
        let adv = block.model_mix.frontier_advisory.as_ref().unwrap();
        assert!(
            adv.contains("community heuristic"),
            "must name basis: {adv}"
        );
        assert!(adv.contains("25%"), "must name threshold: {adv}");
    }

    /// Golden: exactly 10% cheap share should NOT fire advisory (threshold is
    /// strictly under 10%).
    #[test]
    fn model_mix_cheap_exactly_10pct_no_advisory() {
        // haiku = 0.40 / (0.40 + 3.60) = 10.0%
        let measured = measured_two_models("claude-opus-4", 3.60, "claude-haiku-3-5", 0.40, 100);
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &duo_cost,
            input_rate_fn: &duo_rates,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        assert!(
            block.model_mix.cheap_advisory.is_none(),
            "10% exactly must not fire: {:?}",
            block.model_mix.cheap_advisory
        );
    }

    /// Golden: under 10% cheap fires advisory.
    #[test]
    fn model_mix_cheap_under_10pct_fires_advisory() {
        // haiku = 0.39 / (3.61 + 0.39) = 9.75%
        let measured = measured_two_models("claude-opus-4", 3.61, "claude-haiku-3-5", 0.39, 100);
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &duo_cost,
            input_rate_fn: &duo_rates,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        assert!(
            block.model_mix.cheap_advisory.is_some(),
            "under 10% must fire advisory"
        );
        let adv = block.model_mix.cheap_advisory.as_ref().unwrap();
        assert!(
            adv.contains("community heuristic"),
            "must name basis: {adv}"
        );
        assert!(adv.contains("10%"), "must name threshold: {adv}");
    }

    /// Golden: no priced spend yields no advisories.
    #[test]
    fn model_mix_no_priced_spend_no_advisories() {
        let mut by_model = BTreeMap::new();
        by_model.insert(
            "some-model".to_string(),
            ModelUsage {
                totals: usage(1000, 200, 0, 0, 0),
                cost_usd: None,
            },
        );
        let measured = Measured {
            label: "ground truth",
            sessions: 1,
            first_ts: Some(1000),
            last_ts: Some(2000),
            totals: usage(1000, 200, 0, 0, 0),
            by_model,
            cost_usd_total: 0.0,
            unpriced_models: vec![UnpricedModel {
                model: "some-model".to_string(),
                tokens: 1200,
            }],
            cache_hit_rate: 0.0,
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &no_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        assert!(block.model_mix.frontier.is_none());
        assert!(block.model_mix.frontier_advisory.is_none());
        assert!(block.model_mix.cheap_advisory.is_none());
        assert!(block.cap_runway.is_none());
    }

    /// Golden: single model covers the edge case where frontier == cheap.
    #[test]
    fn model_mix_single_model_no_split() {
        let mut by_model = BTreeMap::new();
        by_model.insert(
            "claude-opus-4".to_string(),
            ModelUsage {
                totals: usage(1000, 200, 0, 0, 0),
                cost_usd: Some(5.0),
            },
        );
        let measured = Measured {
            label: "ground truth",
            sessions: 1,
            first_ts: Some(1000),
            last_ts: Some(2000),
            totals: usage(1000, 200, 0, 0, 0),
            by_model,
            cost_usd_total: 5.0,
            unpriced_models: vec![],
            cache_hit_rate: 0.0,
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &duo_cost,
            input_rate_fn: &duo_rates,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        // Single model: frontier share = 100%, but cheap share also = 100%,
        // so cheap advisory does NOT fire (100% >= 10%).
        // frontier_advisory DOES fire (100% > 25%).
        assert!(
            block.model_mix.frontier_advisory.is_some(),
            "100% frontier should fire"
        );
        assert!(
            block.model_mix.cheap_advisory.is_none(),
            "single model at 100% should not fire cheap advisory"
        );
    }

    /// Golden: output share is computed correctly and framing sentence present.
    #[test]
    fn output_share_framing_present() {
        let measured = measured_two_models("claude-opus-4", 3.0, "claude-haiku-3-5", 1.0, 200);
        // cost_fn: return 0 for input-side, so output cost is directly known.
        fn out_cost_fn(model: &str, t: &UsageTotals) -> Option<f64> {
            // Only count output.
            let rate = match model {
                "claude-opus-4" => 75.0 / 1_000_000.0,
                "claude-haiku-3-5" => 4.0 / 1_000_000.0,
                _ => return None,
            };
            Some(t.output_tokens as f64 * rate)
        }
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &out_cost_fn,
            input_rate_fn: &no_rate,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        let os = &block.output_share;
        assert_eq!(os.output_tokens, 200);
        assert!(
            os.framing
                .contains("most any output-side tool could possibly save"),
            "framing sentence missing: {}",
            os.framing
        );
        assert_eq!(os.label, "ground truth");
    }

    // -------------------------------------------------------------------------
    // Cap runway tests.

    fn make_session(
        project: &str,
        model: &str,
        last_ts: u64,
        input: u64,
        output: u64,
    ) -> SessionUsage {
        let mut by_model = BTreeMap::new();
        by_model.insert(model.to_string(), usage(input, output, 0, 0, 0));
        let mut by_day = BTreeMap::new();
        by_day.insert(utc_day_from_secs(last_ts), usage(input, output, 0, 0, 0));
        SessionUsage {
            source: UsageSource::ClaudeCode,
            session_id: format!("s-{last_ts}"),
            project_key: project.to_string(),
            first_ts: last_ts - 60,
            last_ts,
            totals: usage(input, output, 0, 0, 0),
            by_model,
            by_day,
            requests: vec![],
        }
    }

    // Cost function: $3.00/MTok input, $15.00/MTok output.
    fn simple_cost(model: &str, t: &UsageTotals) -> Option<f64> {
        if model == "test-model" {
            Some(
                t.input_tokens as f64 * 3.0 / 1_000_000.0
                    + t.output_tokens as f64 * 15.0 / 1_000_000.0,
            )
        } else {
            None
        }
    }

    /// Golden: cap unset produces no runway block.
    #[test]
    fn cap_runway_absent_when_no_cap() {
        let measured = measured_two_models("test-model", 2.0, "claude-haiku-3-5", 0.5, 100);
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &simple_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: None,
            now: 1_780_000_000,
        };
        let block = compute(&inputs);
        assert!(block.cap_runway.is_none());
        // JSON serialization must omit the key entirely.
        let val = serde_json::to_value(&block).unwrap();
        assert!(
            val["cap_runway"].is_null() || !val.as_object().unwrap().contains_key("cap_runway"),
            "cap_runway must be absent from JSON when unset"
        );
    }

    /// Golden: cap already exceeded renders the exceeded state.
    #[test]
    fn cap_runway_cap_already_exceeded() {
        // "2026-06-10" = 1749513600 in UTC.
        let june10 = 1_749_513_600u64;
        // Build 5 sessions on different days in June, each costing $0.50
        // (model with 1M input tokens at $3.0/MTok = $3.00? let's do smaller).
        // We need MTD spend > cap. 5 sessions * $3/session = $15 MTD > $10 cap.
        let sessions: Vec<SessionUsage> = (0..5)
            .map(|i| make_session("/proj", "test-model", june10 - (i * 86_400), 1_000_000, 0))
            .collect();
        let measured = {
            let mut by_model = BTreeMap::new();
            by_model.insert(
                "test-model".to_string(),
                ModelUsage {
                    totals: usage(5_000_000, 0, 0, 0, 0),
                    cost_usd: Some(15.0), // $3 * 5
                },
            );
            Measured {
                label: "ground truth",
                sessions: 5,
                first_ts: Some(june10 - 4 * 86_400),
                last_ts: Some(june10),
                totals: usage(5_000_000, 0, 0, 0, 0),
                by_model,
                cost_usd_total: 15.0,
                unpriced_models: vec![],
                cache_hit_rate: 0.0,
            }
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &sessions,
            cost_fn: &simple_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: Some(10.0),
            now: june10,
        };
        let block = compute(&inputs);
        let cap = block.cap_runway.as_ref().expect("cap_runway present");
        assert!(cap.cap_already_reached, "cap should be reached");
        assert!(
            cap.rate_7d
                .projection_note
                .as_deref()
                .unwrap_or("")
                .contains("cap already reached"),
            "7d note: {:?}",
            cap.rate_7d.projection_note
        );
        assert!(
            cap.rate_30d
                .projection_note
                .as_deref()
                .unwrap_or("")
                .contains("cap already reached"),
            "30d note: {:?}",
            cap.rate_30d.projection_note
        );
    }

    /// Golden: fewer than 3 active days in window yields insufficient-data note.
    #[test]
    fn cap_runway_fewer_than_3_active_days() {
        // 2 sessions = 2 active days; 7-day window needs >= 3.
        let june10 = 1_749_513_600u64;
        let sessions: Vec<SessionUsage> = (0..2)
            .map(|i| make_session("/proj", "test-model", june10 - (i * 86_400), 100_000, 0))
            .collect();
        let measured = {
            let mut by_model = BTreeMap::new();
            by_model.insert(
                "test-model".to_string(),
                ModelUsage {
                    totals: usage(200_000, 0, 0, 0, 0),
                    cost_usd: Some(0.60),
                },
            );
            Measured {
                label: "ground truth",
                sessions: 2,
                first_ts: Some(june10 - 86_400),
                last_ts: Some(june10),
                totals: usage(200_000, 0, 0, 0, 0),
                by_model,
                cost_usd_total: 0.60,
                unpriced_models: vec![],
                cache_hit_rate: 0.0,
            }
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &sessions,
            cost_fn: &simple_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: Some(50.0),
            now: june10,
        };
        let block = compute(&inputs);
        let cap = block.cap_runway.as_ref().expect("cap_runway present");
        assert!(!cap.cap_already_reached);
        // 7-day window: 2 active days < 3, so note fires.
        assert!(
            cap.rate_7d.projection_note.is_some(),
            "fewer than 3 days must yield projection note for 7d"
        );
        assert!(
            cap.rate_7d.projected_cap_date.is_none(),
            "no projected date when insufficient data"
        );
    }

    /// Golden: valid 7-day projection with enough active days.
    #[test]
    fn cap_runway_valid_projection_7d() {
        // 4 active days, $1 per day, cap $50, MTD $4 -> remaining $46.
        // 7-day avg = (4 * 1.0) / 7 = 0.5714 $/day.
        // days_to_cap = ceil(46 / 0.5714) = ceil(80.5) = 81 days from today.
        let june10 = 1_749_513_600u64;
        let sessions: Vec<SessionUsage> = (0..4)
            .map(|i| make_session("/proj", "test-model", june10 - (i * 86_400), 333_333, 0))
            .collect();
        // Each session: 333,333 input at $3/MTok = $0.999.. ~ $1.
        let total_cost: f64 = 4.0 * (333_333.0 * 3.0 / 1_000_000.0);
        let measured = {
            let mut by_model = BTreeMap::new();
            by_model.insert(
                "test-model".to_string(),
                ModelUsage {
                    totals: usage(333_333 * 4, 0, 0, 0, 0),
                    cost_usd: Some(total_cost),
                },
            );
            Measured {
                label: "ground truth",
                sessions: 4,
                first_ts: Some(june10 - 3 * 86_400),
                last_ts: Some(june10),
                totals: usage(333_333 * 4, 0, 0, 0, 0),
                by_model,
                cost_usd_total: total_cost,
                unpriced_models: vec![],
                cache_hit_rate: 0.0,
            }
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &sessions,
            cost_fn: &simple_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: Some(50.0),
            now: june10,
        };
        let block = compute(&inputs);
        let cap = block.cap_runway.as_ref().expect("cap_runway present");
        assert!(!cap.cap_already_reached);
        assert_eq!(cap.rate_7d.active_days, 4);
        assert!(
            cap.rate_7d.projected_cap_date.is_some(),
            "projection should be available with 4 active days"
        );
        // The projection date string must be a valid "YYYY-MM-DD".
        let date = cap.rate_7d.projected_cap_date.as_ref().unwrap();
        assert_eq!(date.len(), 10, "date must be YYYY-MM-DD: {date}");
        assert!(date.contains('-'), "date must be YYYY-MM-DD: {date}");
    }

    /// day_str round-trip test.
    #[test]
    fn day_str_round_trip() {
        let secs_samples = [0u64, 86_400, 1_704_067_200, 1_749_513_600];
        for s in secs_samples {
            let day = utc_day_from_secs(s);
            let back = day_str_to_secs(&day);
            assert_eq!(
                back,
                (s / 86_400) * 86_400,
                "round-trip failed for {s}: {day}"
            );
        }
    }

    /// TUI compact lines: both lines present when cap configured.
    #[test]
    fn tui_compact_lines_cap_and_output() {
        let june10 = 1_749_513_600u64;
        // 4 active days, then generate the block with a cap.
        let sessions: Vec<SessionUsage> = (0..4)
            .map(|i| make_session("/proj", "test-model", june10 - (i * 86_400), 333_333, 100))
            .collect();
        let total_input: u64 = 333_333 * 4;
        let total_output: u64 = 100 * 4;
        let total_cost: f64 =
            total_input as f64 * 3.0 / 1_000_000.0 + total_output as f64 * 15.0 / 1_000_000.0;
        let mut by_model = BTreeMap::new();
        by_model.insert(
            "test-model".to_string(),
            ModelUsage {
                totals: usage(total_input, total_output, 0, 0, 0),
                cost_usd: Some(total_cost),
            },
        );
        let measured = Measured {
            label: "ground truth",
            sessions: 4,
            first_ts: Some(june10 - 3 * 86_400),
            last_ts: Some(june10),
            totals: usage(total_input, total_output, 0, 0, 0),
            by_model,
            cost_usd_total: total_cost,
            unpriced_models: vec![],
            cache_hit_rate: 0.0,
        };
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &sessions,
            cost_fn: &simple_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: Some(50.0),
            now: june10,
        };
        let block = compute(&inputs);
        let lines = tui_compact_lines(&block);
        // At least 1 line (output share); up to 2 (cap runway if projection).
        assert!(!lines.is_empty(), "must have at least one TUI line");
        assert!(lines.len() <= 2, "at most two TUI lines");
        assert!(
            lines[0].contains("output share"),
            "first line must be output share: {:?}",
            lines
        );

        // Detail sections stay index-aligned with the compact lines: same
        // count, same order, full framing copy in the first paragraph.
        let sections = tui_detail_sections(&block);
        assert_eq!(sections.len(), lines.len(), "sections align to lines");
        assert_eq!(sections[0].title, "output share");
        assert!(
            sections[0].paragraphs[0].contains("Output tokens are"),
            "framing sentence missing: {:?}",
            sections[0].paragraphs
        );
        if sections.len() == 2 {
            assert_eq!(sections[1].title, "cap runway");
            assert!(
                sections[1].paragraphs.iter().any(|p| p.contains("MTD")),
                "cap detail missing MTD line: {:?}",
                sections[1].paragraphs
            );
        }
    }

    /// Detail sections mirror the compact-line inclusion conditions when no
    /// priced spend exists: both surfaces stay empty together.
    #[test]
    fn tui_detail_sections_empty_without_priced_bill() {
        let measured = measured_two_models("m1", 0.0, "m2", 0.0, 100);
        let inputs = AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &no_cost,
            input_rate_fn: &no_rate,
            monthly_cap_usd: None,
            now: 2_000,
        };
        let block = compute(&inputs);
        // cost_usd_total is 0 here, so the output-share line is suppressed.
        assert_eq!(tui_compact_lines(&block).len(), 0);
        assert_eq!(tui_detail_sections(&block).len(), 0);
    }
}
