//! `tolkin report --html`: stakeholder artifact.
//!
//! Renders a self-contained static HTML file (project or machine scope) for
//! sharing with non-technical readers. Inputs come exclusively from the same
//! `StatsSnapshot` the `stats` command and the TUI consume, which is the
//! mechanism that keeps the three surfaces in agreement.
//!
//! Privacy posture: the document contains only headline numbers (project key,
//! tier totals, daily aggregates). It inherits the no-content, no-secret
//! guarantee from the underlying `StatsSnapshot`; nothing new is exposed here.
//! No external requests of any kind: no CDN scripts, no web fonts, no images,
//! no JavaScript. Inline CSS only, system font stack, printable on A4/Letter.
//!
//! Note: reading the ledger to render a report is not a "write" and is not
//! gated by `CI=true` / `TOLKIN_NO_LEDGER`; that env kill governs writes and
//! log ingestion, mirroring how `stats` behaves.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Args;
use tolkin_core::pricing;

use crate::advisories::AdvisoryBlock;
use crate::cache_analysis::CacheReport;
use crate::ledger;
use crate::tiers::{Identified, Measured, Realized, TierReport};
use crate::tui::data;

use super::stats_data::StatsSnapshot;

const DEFAULT_OUTPUT: &str = "tolkin-report.html";

#[derive(Args, Debug)]
pub struct ReportArgs {
    /// Machine-wide report across every project in the ledger.
    #[arg(long)]
    pub global: bool,

    /// Render as a self-contained HTML file. Currently the only format; the
    /// flag exists so a future `--json` can join it without breaking scripts.
    #[arg(long)]
    pub html: bool,

    /// Output path. Defaults to ./tolkin-report.html in the current directory.
    #[arg(long, short = 'o', value_name = "PATH")]
    pub output: Option<PathBuf>,
}

pub fn run(args: ReportArgs) -> Result<()> {
    let dir = ledger::data_dir()
        .ok_or_else(|| anyhow!("no data directory available; run `tolkin init` first"))?;
    let snapshot = StatsSnapshot::load_in(&dir);

    let output = args.output.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    let html = render_html(&snapshot, args.global);
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&output, html)?;
    let abs = output.canonicalize().unwrap_or(output);
    println!("{}", abs.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// HTML construction.

/// Build the entire HTML document. Pure function over the snapshot and scope
/// flag so tests can render any fixture without touching the filesystem.
pub fn render_html(snapshot: &StatsSnapshot, global: bool) -> String {
    let scope_project = if global {
        None
    } else {
        Some(snapshot.project_key.as_str())
    };
    let report = if global {
        snapshot.compute_global()
    } else {
        snapshot.compute_project()
    };
    let scope_label = match scope_project {
        Some(p) => format!("project: {}", p),
        None => "all projects (machine-wide)".to_string(),
    };
    let generated_at = format_utc(snapshot.now);
    let version = env!("CARGO_PKG_VERSION");

    let mut body = String::with_capacity(8 * 1024);
    let advisories = snapshot.compute_advisories(&report);
    body.push_str(&render_header(&scope_label, &generated_at, version));
    body.push_str(&render_tier_identified(report.identified.as_ref(), global));
    body.push_str(&render_tier_realized(
        report.realized.as_ref(),
        snapshot.rate_model_display,
    ));
    body.push_str(&render_tier_measured(report.measured.as_ref()));
    // The cache health section consumes the same snapshot/analysis the
    // `tolkin cache` command does, so the two surfaces can never disagree.
    if let Some(cache) = snapshot.compute_cache(scope_project) {
        body.push_str(&render_cache_section(&cache));
    }
    // Advisory section: model mix, output share, cap runway.
    if let Some(ref adv) = advisories {
        body.push_str(&render_advisories_section(adv));
    }
    if let Some(measured) = report.measured.as_ref() {
        if measured.sessions > 0 {
            body.push_str(&render_spend_trend(snapshot));
        }
    }
    if let Some(p) = scope_project {
        body.push_str(&render_load_profile(snapshot, p));
    }
    body.push_str(&render_methodology(&report, snapshot));

    wrap_document(body)
}

fn wrap_document(body: String) -> String {
    let mut out = String::with_capacity(body.len() + STYLE.len() + 1024);
    out.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    out.push_str("<title>Tolkin savings report</title>");
    out.push_str("<style>");
    out.push_str(STYLE);
    out.push_str("</style></head><body><main class=\"page\">");
    out.push_str(&body);
    out.push_str("</main></body></html>\n");
    out
}

fn render_header(scope_label: &str, generated_at: &str, version: &str) -> String {
    format!(
        "<header class=\"hdr\">\
<h1>Tolkin savings report</h1>\
<p class=\"meta\"><span class=\"meta-label\">Scope</span> {scope}</p>\
<p class=\"meta\"><span class=\"meta-label\">Generated</span> {generated}</p>\
<p class=\"meta\"><span class=\"meta-label\">Tolkin</span> v{version}</p>\
<p class=\"meta\"><span class=\"meta-label\">Prices observed</span> {prices}</p>\
</header>",
        scope = html_escape(scope_label),
        generated = html_escape(generated_at),
        version = html_escape(version),
        prices = html_escape(pricing::PRICES_OBSERVED),
    )
}

fn render_tier_identified(identified: Option<&Identified>, global: bool) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"tier\" id=\"identified\"><div class=\"card\">");
    out.push_str("<div class=\"label\">Identified</div>");
    out.push_str("<h2>Reclaimable right now</h2>");
    match identified {
        Some(i) => {
            out.push_str(&format!(
                "<p class=\"tier-sub\">{}</p>",
                html_escape(i.label)
            ));
            out.push_str("<table class=\"kv\"><tbody>");
            if let (Some(min), Some(max)) = (i.project_reclaimable_min, i.project_reclaimable_max) {
                out.push_str(&kv_row(
                    "Project reclaimable",
                    &format!("{} to {} tokens", commas(min), commas(max)),
                    i.project_as_of,
                ));
            }
            if let (Some(swap), Some(slim)) = (i.mcp_swap_tokens, i.mcp_slim_tokens) {
                out.push_str(&kv_row(
                    "MCP swap (CLI swap)",
                    &format!("{} tokens", commas(swap)),
                    i.mcp_as_of,
                ));
                out.push_str(&kv_row(
                    "MCP slim profile",
                    &format!("{} tokens", commas(slim)),
                    i.mcp_as_of,
                ));
            }
            if let (Some(min), Some(max)) = (i.audit_savings_min, i.audit_savings_max) {
                out.push_str(&kv_row(
                    "Audit findings",
                    &format!("{} to {} tokens", commas(min), commas(max)),
                    i.audit_as_of,
                ));
            }
            if global {
                out.push_str(&kv_row("Projects contributing", &commas(i.projects), None));
            }
            out.push_str("</tbody></table>");
        }
        None => {
            out.push_str("<p class=\"empty\">No identified savings recorded yet. Run <code>tolkin project</code>, <code>tolkin scan</code>, or <code>tolkin audit</code> to populate this section.</p>");
        }
    }
    out.push_str("</div></section>");
    out
}

fn render_tier_realized(realized: Option<&Realized>, rate_display: &str) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"tier\" id=\"realized\"><div class=\"card\">");
    out.push_str("<div class=\"label\">Realized</div>");
    out.push_str("<h2>Standing context delta, applied to real sessions</h2>");
    match realized {
        Some(r) => {
            out.push_str(&format!(
                "<p class=\"tier-sub\">{}</p>",
                html_escape(r.label)
            ));
            let sign = if r.tokens < 0 { "-" } else { "" };
            let token_str = format!("{}{} input tokens", sign, commas(r.tokens.unsigned_abs()));
            let usd_str = format!("{}{}", sign, usd(r.usd.abs()));
            out.push_str("<table class=\"kv\"><tbody>");
            out.push_str(&kv_row("Realized tokens", &token_str, None));
            out.push_str(&kv_row(
                "Realized dollars",
                &format!("{} at {} input rates", usd_str, html_escape(rate_display)),
                None,
            ));
            out.push_str(&kv_row(
                "Sessions counted",
                &format!(
                    "{} ({} basis)",
                    commas(r.sessions_count),
                    html_escape(r.sessions_basis)
                ),
                None,
            ));
            out.push_str(&kv_row(
                "Always-loaded context",
                &format!(
                    "{} to {} tokens",
                    commas(r.baseline_always_tokens),
                    commas(r.current_always_tokens)
                ),
                None,
            ));
            out.push_str(&kv_row("Since", &format_utc(r.since), None));
            if r.tokens < 0 {
                out.push_str("<tr><td colspan=\"2\" class=\"warn\">Standing context grew between snapshots. Realized savings are reported as negative, never hidden or clamped.</td></tr>");
            }
            out.push_str("</tbody></table>");
        }
        None => {
            out.push_str("<p class=\"empty\">Two or more project snapshots are required to compute realized savings. Re-run <code>tolkin project</code> after structural changes.</p>");
        }
    }
    out.push_str("</div></section>");
    out
}

fn render_tier_measured(measured: Option<&Measured>) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"tier\" id=\"measured\"><div class=\"card\">");
    out.push_str("<div class=\"label\">Measured</div>");
    out.push_str("<h2>Actual spend from local agent session logs</h2>");
    match measured {
        Some(m) => {
            out.push_str(&format!(
                "<p class=\"tier-sub\">{}</p>",
                html_escape(m.label)
            ));
            let span = match (m.first_ts, m.last_ts) {
                (Some(f), Some(l)) => format!("{} to {}", format_utc(f), format_utc(l)),
                _ => "no sessions in scope".to_string(),
            };
            out.push_str("<table class=\"kv\"><tbody>");
            out.push_str(&kv_row("Sessions", &commas(m.sessions), None));
            out.push_str(&kv_row("Date span", &span, None));
            out.push_str(&kv_row(
                "Fresh input tokens",
                &commas(m.totals.input_tokens),
                None,
            ));
            out.push_str(&kv_row(
                "Cache read tokens",
                &commas(m.totals.cache_read_tokens),
                None,
            ));
            out.push_str(&kv_row(
                "Cache write tokens",
                &commas(m.totals.cache_write_5m_tokens + m.totals.cache_write_1h_tokens),
                None,
            ));
            out.push_str(&kv_row(
                "Output tokens",
                &commas(m.totals.output_tokens),
                None,
            ));
            out.push_str(&kv_row(
                "Cache hit rate",
                &format!("{:.1}% of input-side tokens", m.cache_hit_rate * 100.0),
                None,
            ));
            out.push_str(&kv_row(
                "Cost (priced models)",
                &format!(
                    "{} (prices observed {})",
                    usd(m.cost_usd_total),
                    html_escape(pricing::PRICES_OBSERVED)
                ),
                None,
            ));
            out.push_str("</tbody></table>");
            out.push_str(&render_model_table(m));
            if !m.unpriced_models.is_empty() {
                out.push_str("<h3 class=\"sub\">Unpriced models</h3>");
                out.push_str("<table class=\"kv\"><tbody>");
                for u in &m.unpriced_models {
                    out.push_str(&kv_row(
                        &u.model,
                        &format!("{} tokens (excluded from cost total)", commas(u.tokens)),
                        None,
                    ));
                }
                out.push_str("</tbody></table>");
            }
        }
        None => {
            out.push_str("<p class=\"empty\">Usage-log ingestion is off. Re-run <code>tolkin init</code> and consent to log ingestion to populate measured ground truth.</p>");
        }
    }
    out.push_str("</div></section>");
    out
}

fn render_model_table(measured: &Measured) -> String {
    if measured.by_model.is_empty() {
        return String::new();
    }
    // Top models by input-side volume; the table is right-aligned numerals so
    // a glance gives the relative footprint.
    let mut rows: Vec<(&String, u64, u64, Option<f64>)> = measured
        .by_model
        .iter()
        .map(|(model, mu)| {
            (
                model,
                mu.totals.input_side(),
                mu.totals.output_tokens,
                mu.cost_usd,
            )
        })
        .collect();
    rows.sort_by_key(|&(_, input_side, _, _)| std::cmp::Reverse(input_side));
    let mut out = String::new();
    out.push_str("<h3 class=\"sub\">By model</h3>");
    out.push_str("<table class=\"data\"><thead><tr><th>Model</th><th class=\"num\">Input side</th><th class=\"num\">Output</th><th class=\"num\">Cost</th></tr></thead><tbody>");
    for (model, input_side, output, cost) in rows.iter().take(8) {
        let cost_str = match cost {
            Some(c) => usd(*c),
            None => "unpriced".to_string(),
        };
        out.push_str(&format!(
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            html_escape(model),
            commas(*input_side),
            commas(*output),
            html_escape(&cost_str),
        ));
    }
    out.push_str("</tbody></table>");
    out
}

/// Prompt-cache health, mirroring `tolkin cache` (same analysis, same
/// labels). Renders only when ingestion is on, like the measured tier.
fn render_cache_section(cache: &CacheReport) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"tier\" id=\"cache\"><div class=\"card\">");
    out.push_str("<div class=\"label\">Cache</div>");
    out.push_str("<h2>Prompt cache health</h2>");
    out.push_str(&format!(
        "<p class=\"tier-sub\">Claude Code session logs only ({} sessions, {} requests); Codex rollouts carry no cache-write fields.</p>",
        commas(cache.sessions_analyzed),
        commas(cache.requests_analyzed),
    ));

    out.push_str("<table class=\"kv\"><tbody>");
    let hr = &cache.hit_rate;
    out.push_str(&kv_row(
        "Hit rate (ground truth)",
        &format!(
            "{:.1}% ({} cache-read of {} input-side tokens)",
            hr.rate * 100.0,
            commas(hr.cache_read_tokens),
            commas(hr.input_side_tokens)
        ),
        None,
    ));
    if let Some(advisory) = &hr.advisory {
        out.push_str(&format!(
            "<tr><td colspan=\"2\" class=\"warn\">{}</td></tr>",
            html_escape(advisory)
        ));
    }
    let churn = &cache.write_churn;
    out.push_str(&kv_row(
        "Write churn (ground truth)",
        &format!(
            "{:.1}% of write tokens written after a session's first write ({} of {})",
            churn.share * 100.0,
            commas(churn.writes_after_first_tokens),
            commas(churn.total_write_tokens)
        ),
        None,
    ));
    let ttl = &cache.ttl_counterfactual;
    out.push_str(&kv_row(
        "Observed cache writes (ground truth)",
        &format!(
            "{} tokens at the 5m TTL, {} at the 1h TTL",
            commas(ttl.observed_write_tokens_5m),
            commas(ttl.observed_write_tokens_1h)
        ),
        None,
    ));
    out.push_str(&kv_row(
        "Simulated 5m strategy (advisory estimate)",
        &format!(
            "W5 = {} write events, {} write tokens",
            commas(ttl.simulated_w5_write_events),
            commas(ttl.simulated_w5_write_tokens)
        ),
        None,
    ));
    out.push_str(&kv_row(
        "Simulated 1h strategy (advisory estimate)",
        &format!(
            "W1 = {} write events, {} write tokens",
            commas(ttl.simulated_w1_write_events),
            commas(ttl.simulated_w1_write_tokens)
        ),
        None,
    ));
    out.push_str(&kv_row(
        "Marginal dollars (advisory estimate)",
        &format!(
            "5m strategy {}, 1h strategy {} (priced models only)",
            usd(ttl.usd_5m_strategy),
            usd(ttl.usd_1h_strategy)
        ),
        None,
    ));
    let cadence = &cache.cadence;
    out.push_str(&kv_row(
        "Cadence (ground truth)",
        &format!(
            "{:.1}% of intra-session gaps over 5 minutes; {:.1}% of inter-session gaps (same project) under an hour; {} session(s) with zero cache reads",
            cadence.share_intra_gaps_over_5m * 100.0,
            cadence.share_inter_gaps_under_1h * 100.0,
            commas(cadence.sessions_zero_cache_read)
        ),
        None,
    ));
    out.push_str("</tbody></table>");

    if !churn.worst_sessions.is_empty() {
        out.push_str("<h3 class=\"sub\">Worst churn sessions</h3>");
        out.push_str("<table class=\"data\"><thead><tr><th>Session</th><th>Started</th><th class=\"num\">After first</th><th class=\"num\">Total writes</th><th class=\"num\">Share of writes</th></tr></thead><tbody>");
        for w in &churn.worst_sessions {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>",
                html_escape(&w.session_id),
                html_escape(&format_utc(w.first_ts)),
                commas(w.writes_after_first_tokens),
                commas(w.write_tokens),
                w.share * 100.0,
            ));
        }
        out.push_str("</tbody></table>");
    }

    out.push_str(&format!(
        "<p class=\"note\">{}</p>",
        html_escape(crate::cache_analysis::CHURN_NOTE)
    ));
    out.push_str(&format!(
        "<p class=\"formula\">Break even: {}. {}</p>",
        html_escape(ttl.break_even),
        html_escape(&ttl.tier_note)
    ));
    out.push_str(&format!(
        "<p class=\"note\">{}</p>",
        html_escape(&ttl.verdict)
    ));
    if !ttl.unpriced_models.is_empty() {
        out.push_str(&format!(
            "<p class=\"note\">Dollar figures cover priced models only; unpriced: {}.</p>",
            html_escape(&ttl.unpriced_models.join(", "))
        ));
    }
    out.push_str(&format!(
        "<p class=\"honesty\">{}</p>",
        html_escape(cache.scope_line)
    ));
    out.push_str("</div></section>");
    out
}

/// Measured advisories section: model mix, output share, and cap runway.
/// Rendered only when the measured tier is present (same gate the TUI uses).
fn render_advisories_section(block: &AdvisoryBlock) -> String {
    let mut out = String::new();
    out.push_str("<section class=\"tier\" id=\"advisories\"><div class=\"card\">");
    out.push_str("<div class=\"label\">Advisories</div>");
    out.push_str("<h2>Measured advisories</h2>");
    out.push_str(&format!(
        "<p class=\"tier-sub\">{}</p>",
        html_escape(block.label)
    ));
    out.push_str("<table class=\"kv\"><tbody>");

    // Model mix.
    let mix = &block.model_mix;
    out.push_str(&kv_row(
        "Model mix",
        &format!("({})", html_escape(mix.label)),
        None,
    ));
    if mix.priced_spend_total > 0.0 {
        if let Some(ref f) = mix.frontier {
            out.push_str(&kv_row(
                "Top model",
                &format!(
                    "{}: {:.1}% of priced spend ({})",
                    html_escape(&f.model),
                    f.share * 100.0,
                    usd(f.spend_usd)
                ),
                None,
            ));
        }
        if let Some(ref c) = mix.cheap {
            out.push_str(&kv_row(
                "Cheapest-tier model",
                &format!(
                    "{}: {:.1}% of priced spend ({})",
                    html_escape(&c.model),
                    c.share * 100.0,
                    usd(c.spend_usd)
                ),
                None,
            ));
        }
        if !mix.unpriced_models_excluded.is_empty() {
            out.push_str(&kv_row(
                "Unpriced models (excluded from mix)",
                &html_escape(&mix.unpriced_models_excluded.join(", ")),
                None,
            ));
        }
        if let Some(ref adv) = mix.frontier_advisory {
            out.push_str(&format!(
                "<tr><td colspan=\"2\" class=\"note\">{}</td></tr>",
                html_escape(adv)
            ));
        }
        if let Some(ref adv) = mix.cheap_advisory {
            out.push_str(&format!(
                "<tr><td colspan=\"2\" class=\"note\">{}</td></tr>",
                html_escape(adv)
            ));
        }
    } else {
        out.push_str(&kv_row("Model mix", "no priced spend recorded", None));
    }

    // Output share.
    let os = &block.output_share;
    out.push_str(&kv_row(
        "Output tokens",
        &format!(
            "{} ({:.1}% of priced bill)",
            commas(os.output_tokens),
            os.share * 100.0
        ),
        None,
    ));
    out.push_str(&kv_row(
        "Output cost",
        &format!(
            "{} of {} priced bill",
            usd(os.output_cost_usd),
            usd(os.priced_bill_usd)
        ),
        None,
    ));
    out.push_str(&format!(
        "<tr><td colspan=\"2\" class=\"note\">{}</td></tr>",
        html_escape(&os.framing)
    ));

    // Cap runway.
    if let Some(ref cap) = block.cap_runway {
        out.push_str(&kv_row(
            "Monthly cap",
            &format!("${:.2}", cap.monthly_cap_usd),
            None,
        ));
        out.push_str(&kv_row("MTD spend", &usd(cap.mtd_spend_usd), None));
        if cap.cap_already_reached {
            out.push_str("<tr><td colspan=\"2\" class=\"warn\">Monthly cap already reached this month.</td></tr>");
        } else {
            out.push_str(&kv_row("Remaining", &usd(cap.remaining_usd), None));
            if let Some(ref date) = cap.rate_7d.projected_cap_date {
                out.push_str(&kv_row(
                    "7-day projection",
                    &format!(
                        "at {} avg/day, cap reached {}",
                        usd(cap.rate_7d.avg_daily_usd),
                        html_escape(date)
                    ),
                    None,
                ));
            } else if let Some(ref note) = cap.rate_7d.projection_note {
                out.push_str(&kv_row("7-day projection", &html_escape(note), None));
            }
            if let Some(ref date) = cap.rate_30d.projected_cap_date {
                out.push_str(&kv_row(
                    "30-day projection",
                    &format!(
                        "at {} avg/day, cap reached {}",
                        usd(cap.rate_30d.avg_daily_usd),
                        html_escape(date)
                    ),
                    None,
                ));
            } else if let Some(ref note) = cap.rate_30d.projection_note {
                out.push_str(&kv_row("30-day projection", &html_escape(note), None));
            }
        }
    }

    out.push_str("</tbody></table>");
    out.push_str("</div></section>");
    out
}

fn render_spend_trend(snapshot: &StatsSnapshot) -> String {
    // Pure-CSS bar chart: 30 UTC days of input-side aggregates. Heights are
    // computed in Rust so the document needs zero JavaScript. The day-merge
    // logic is reused from the TUI's `data` module to keep both surfaces in
    // agreement (see the flagged tui/mod.rs visibility exception).
    let today = data::day_string(snapshot.now);
    let bars = data::spend_day_bars(snapshot, &today);
    if bars.is_empty() {
        return String::new();
    }
    let max = bars.iter().map(|b| b.input_side).max().unwrap_or(0);
    let mut out = String::new();
    out.push_str("<section class=\"trend\"><div class=\"card\">");
    out.push_str("<h2>Spend trend (last 30 UTC days)</h2>");
    out.push_str(
        "<p class=\"tier-sub\">Input-side tokens per UTC day, from ingested session logs.</p>",
    );
    out.push_str("<div class=\"chart\" role=\"img\" aria-label=\"30 day input token volume\">");
    for bar in &bars {
        let pct = bar_height_pct(bar.input_side, max);
        out.push_str(&format!(
            "<div class=\"bar\" title=\"{}: {} input tokens\"><span class=\"bar-fill\" style=\"height: {:.2}%\"></span></div>",
            html_escape(&bar.day),
            commas(bar.input_side),
            pct,
        ));
    }
    out.push_str("</div>");
    out.push_str(&format!(
        "<p class=\"axis\"><span>{}</span><span>{}</span></p>",
        html_escape(bars.first().map(|b| b.day.as_str()).unwrap_or("")),
        html_escape(bars.last().map(|b| b.day.as_str()).unwrap_or("")),
    ));
    out.push_str("</div></section>");
    out
}

/// Bar height as a percentage of the chart area. Returns 0.0 when `max` is 0
/// so an all-zero series renders as a flat baseline rather than dividing by
/// zero. The chart container guarantees a minimum height in CSS so even a
/// zero-day bar stays clickable on the rendered page.
pub fn bar_height_pct(value: u64, max: u64) -> f64 {
    if max == 0 {
        return 0.0;
    }
    (value as f64 / max as f64) * 100.0
}

fn render_load_profile(snapshot: &StatsSnapshot, project_key: &str) -> String {
    // The latest project snapshot for this scope, if any. The four headline
    // fields are: always_tokens, on_invocation_tokens, on_demand_tokens,
    // docs_tokens (see commands/project.rs).
    let latest = snapshot
        .records
        .iter()
        .filter(|r| r.command == "project" && r.project_key == project_key)
        .max_by_key(|r| r.ts);
    let Some(record) = latest else {
        return String::new();
    };
    let read = |key: &str| {
        record
            .headline
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    };
    let always = read("always_tokens");
    let on_invocation = read("on_invocation_tokens");
    let on_demand = read("on_demand_tokens");
    let docs = read("docs_tokens");
    if always == 0 && on_invocation == 0 && on_demand == 0 && docs == 0 {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("<section class=\"profile\"><div class=\"card\">");
    out.push_str("<h2>Load profile</h2>");
    out.push_str(&format!(
        "<p class=\"tier-sub\">Latest project snapshot, taken {}.</p>",
        html_escape(&format_utc(record.ts))
    ));
    out.push_str("<table class=\"data\"><thead><tr><th>Profile</th><th class=\"num\">Tokens</th></tr></thead><tbody>");
    out.push_str(&format!(
        "<tr><td>Always loaded</td><td class=\"num\">{}</td></tr>",
        commas(always)
    ));
    out.push_str(&format!(
        "<tr><td>On invocation</td><td class=\"num\">{}</td></tr>",
        commas(on_invocation)
    ));
    out.push_str(&format!(
        "<tr><td>On demand</td><td class=\"num\">{}</td></tr>",
        commas(on_demand)
    ));
    out.push_str(&format!(
        "<tr><td>Docs</td><td class=\"num\">{}</td></tr>",
        commas(docs)
    ));
    out.push_str("</tbody></table>");
    out.push_str("</div></section>");
    out
}

fn render_methodology(report: &TierReport, snapshot: &StatsSnapshot) -> String {
    let assumed_basis = report
        .realized
        .as_ref()
        .is_some_and(|r| r.sessions_basis == "assumed");
    let mut out = String::new();
    out.push_str("<section class=\"foot\"><div class=\"card\">");
    out.push_str("<h2>Methodology</h2>");
    out.push_str("<dl class=\"defs\">");
    out.push_str("<dt>Identified</dt><dd>What audit, scan, mcp, and project flag as reclaimable right now (advisory estimate).</dd>");
    out.push_str("<dt>Realized</dt><dd>The drop in always-loaded context tokens between project snapshots multiplied by the sessions observed since (measured structure, estimated frequency).</dd>");
    out.push_str("<dt>Measured</dt><dd>Actual token spend from local agent session logs, cache-aware (ground truth).</dd>");
    out.push_str("</dl>");
    out.push_str("<p class=\"formula\">Realized formula: for each project, sum across qualifying sessions of (baseline always_tokens minus standing always_tokens at session start). The full ledger at ");
    out.push_str(&format!(
        "<code>{}</code>",
        html_escape(&ledger::ledger_path(&snapshot.dir).display().to_string())
    ));
    out.push_str(" is the audit trail; the formula is reproducible from those records.</p>");
    out.push_str("<p class=\"honesty\">All savings are input-token bounded; output may vary.</p>");
    if assumed_basis {
        out.push_str(&format!(
            "<p class=\"note\">Realized savings on this report use an assumed session rate of {:.2}/day; ingest usage logs (re-run <code>tolkin init</code>) for measured counts.</p>",
            snapshot.assumed_rate
        ));
    }
    out.push_str(&format!(
        "<p class=\"note\">All dollar figures use prices observed {}.</p>",
        html_escape(pricing::PRICES_OBSERVED)
    ));
    for note in &report.notes {
        // Skip the canonical honesty note: it is already rendered as a
        // first-class line above with its own styling.
        if note == "All savings are input-token bounded; output may vary." {
            continue;
        }
        out.push_str(&format!("<p class=\"note\">{}</p>", html_escape(note)));
    }
    out.push_str("</div></section>");
    out
}

fn kv_row(label: &str, value: &str, as_of: Option<u64>) -> String {
    let as_of_str = match as_of {
        Some(ts) => format!(
            " <span class=\"as-of\">as of {}</span>",
            html_escape(&format_utc(ts))
        ),
        None => String::new(),
    };
    format!(
        "<tr><th scope=\"row\">{}</th><td class=\"num\">{}{}</td></tr>",
        html_escape(label),
        value,
        as_of_str,
    )
}

// ---------------------------------------------------------------------------
// Style. One accent color (a steady blue), neutral grays, generous spacing,
// system font stack only. No imports, no JS, no images.

const STYLE: &str = r#"
:root {
  --accent: #2563eb;
  --ink: #111827;
  --ink-muted: #4b5563;
  --rule: #e5e7eb;
  --bg: #ffffff;
  --card-bg: #f9fafb;
  --label-bg: #eff6ff;
  --warn: #92400e;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; background: var(--bg); color: var(--ink); }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  font-size: 15px;
  line-height: 1.55;
}
.page {
  max-width: 920px;
  margin: 0 auto;
  padding: 48px 32px 64px;
}
.hdr { margin-bottom: 32px; }
.hdr h1 { font-size: 28px; margin: 0 0 16px; letter-spacing: -0.01em; }
.meta { margin: 4px 0; color: var(--ink-muted); font-size: 14px; }
.meta-label {
  display: inline-block;
  min-width: 130px;
  color: var(--ink);
  font-weight: 600;
}
section { margin: 24px 0; }
.card {
  background: var(--card-bg);
  border: 1px solid var(--rule);
  border-radius: 8px;
  padding: 24px 28px;
}
.label {
  display: inline-block;
  background: var(--label-bg);
  color: var(--accent);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  padding: 4px 10px;
  border-radius: 999px;
  margin-bottom: 12px;
}
h2 { font-size: 20px; margin: 0 0 8px; letter-spacing: -0.005em; }
h3.sub { font-size: 15px; margin: 20px 0 8px; color: var(--ink-muted); text-transform: uppercase; letter-spacing: 0.06em; }
.tier-sub { margin: 0 0 16px; color: var(--ink-muted); font-size: 14px; font-style: italic; }
table { width: 100%; border-collapse: collapse; margin-top: 8px; }
table.kv th { text-align: left; font-weight: 500; color: var(--ink-muted); padding: 8px 0; vertical-align: top; border-bottom: 1px solid var(--rule); width: 40%; }
table.kv td { text-align: right; padding: 8px 0; border-bottom: 1px solid var(--rule); font-variant-numeric: tabular-nums; }
table.data th { text-align: left; padding: 8px 12px 8px 0; border-bottom: 1px solid var(--rule); color: var(--ink-muted); font-weight: 600; font-size: 13px; }
table.data td { padding: 8px 12px 8px 0; border-bottom: 1px solid var(--rule); font-variant-numeric: tabular-nums; }
.num { text-align: right; font-variant-numeric: tabular-nums; }
.as-of { color: var(--ink-muted); font-size: 12px; font-style: italic; margin-left: 8px; }
.warn { color: var(--warn); font-size: 13px; padding-top: 8px; }
.empty { color: var(--ink-muted); font-style: italic; }
code { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace; font-size: 13px; background: #eef2f7; padding: 1px 6px; border-radius: 4px; }
.chart {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  height: 140px;
  padding: 8px 0;
  border-bottom: 1px solid var(--rule);
}
.bar {
  flex: 1 1 0;
  display: flex;
  align-items: flex-end;
  height: 100%;
  min-width: 4px;
}
.bar-fill {
  display: block;
  width: 100%;
  background: var(--accent);
  border-radius: 2px 2px 0 0;
  min-height: 1px;
}
.axis { display: flex; justify-content: space-between; color: var(--ink-muted); font-size: 12px; margin: 8px 0 0; }
.defs dt { font-weight: 600; margin-top: 8px; }
.defs dd { margin: 0 0 8px 0; color: var(--ink-muted); }
.formula { background: #f3f4f6; border-left: 3px solid var(--accent); padding: 12px 16px; margin: 16px 0; font-size: 14px; }
.honesty { font-weight: 600; color: var(--ink); margin-top: 16px; }
.note { color: var(--ink-muted); font-size: 13px; margin: 6px 0; }
@media print {
  body { background: #ffffff; font-size: 12px; }
  .page { padding: 16px; max-width: none; }
  .card { background: #ffffff; border-color: #cccccc; padding: 16px; break-inside: avoid; page-break-inside: avoid; }
  section { break-inside: avoid; page-break-inside: avoid; margin: 12px 0; }
  .label { background: transparent; border: 1px solid var(--accent); }
  .formula { background: transparent; }
}
"#;

// ---------------------------------------------------------------------------
// Helpers.

/// Minimal HTML attribute and text escaper. The whole document is built by
/// concatenation, so EVERY interpolated value passes through this helper.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Thousands-separated u64 formatting, mirroring the helper in `stats.rs`.
fn commas(n: u64) -> String {
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

/// Dollar formatting that mirrors `commands::stats::usd` so report numbers
/// match plain-text stats exactly.
fn usd(v: f64) -> String {
    if v == 0.0 {
        "$0".to_string()
    } else if v < 0.01 {
        format!("${v:.5}")
    } else if v < 1.0 {
        format!("${v:.3}")
    } else {
        format!("${v:.2}")
    }
}

/// "YYYY-MM-DD HH:MM UTC" formatter, mirrored from `commands::stats`.
fn format_utc(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    let h = rem / 3_600;
    let m = (rem % 3_600) / 60;
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{self, Config, LedgerRecord};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tolkin-report-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn html_escape_replaces_all_metacharacters() {
        assert_eq!(html_escape("plain"), "plain");
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape(r#"he said "hi""#), "he said &quot;hi&quot;");
        assert_eq!(html_escape("it's"), "it&#39;s");
        // A real-world project path containing all the dangerous characters:
        // quotes, angle brackets, ampersand. Every one must be neutralized.
        let evil = "/users/<x>/tmp/\"q\"&p";
        let got = html_escape(evil);
        assert!(!got.contains('<'));
        assert!(!got.contains('>'));
        assert!(!got.contains('"'));
        assert!(got.contains("&lt;"));
        assert!(got.contains("&gt;"));
        assert!(got.contains("&quot;"));
        assert!(got.contains("&amp;"));
    }

    #[test]
    fn bar_height_zero_max_guards_division() {
        // All-zero series: every bar reads zero with no divide-by-zero panic.
        assert_eq!(bar_height_pct(0, 0), 0.0);
        assert_eq!(bar_height_pct(100, 0), 0.0);
        // Zero-day inside a non-zero series stays at 0.
        assert_eq!(bar_height_pct(0, 100), 0.0);
        // Mid range maps linearly to a percentage.
        assert!((bar_height_pct(25, 100) - 25.0).abs() < 1e-9);
        assert!((bar_height_pct(100, 100) - 100.0).abs() < 1e-9);
    }

    /// Append a record directly so tests can seed history without going through
    /// the consent gate (the report itself reads; writes are gated elsewhere).
    fn append_raw(dir: &Path, command: &str, project: &str, ts: u64, headline: serde_json::Value) {
        let rec = LedgerRecord {
            v: 1,
            ts,
            command: command.to_string(),
            project_key: project.to_string(),
            headline,
            tolkin_version: "test".to_string(),
            prices_observed: "test".to_string(),
        };
        let line = serde_json::to_string(&rec).unwrap();
        let path = ledger::ledger_path(dir);
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&line);
        text.push('\n');
        fs::write(&path, text).unwrap();
    }

    #[test]
    fn render_html_contains_tier_labels_and_honesty_line() {
        // Two snapshots so the realized tier is populated, plus an audit and a
        // scan record so identified covers all three components.
        let dir = tmp("full-render");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        append_raw(
            &dir,
            "project",
            "/proj",
            1_780_000_000,
            json!({
                "always_tokens": 12_000,
                "reclaimable_min": 100,
                "reclaimable_max": 400,
                "on_invocation_tokens": 4_000,
                "on_demand_tokens": 1_000,
                "docs_tokens": 200,
            }),
        );
        append_raw(
            &dir,
            "project",
            "/proj",
            1_780_500_000,
            json!({
                "always_tokens": 8_000,
                "reclaimable_min": 100,
                "reclaimable_max": 400,
                "on_invocation_tokens": 4_000,
                "on_demand_tokens": 1_000,
                "docs_tokens": 200,
            }),
        );
        append_raw(
            &dir,
            "audit",
            "/proj",
            1_780_500_001,
            json!({ "savings_min": 50, "savings_max": 90 }),
        );
        append_raw(
            &dir,
            "scan",
            "/proj",
            1_780_500_002,
            json!({ "swap_savings_tokens": 700, "slim_savings_tokens": 300 }),
        );

        let mut snapshot = StatsSnapshot::load_in(&dir);
        // Pin the project_key so we exercise the project-scope load profile
        // section deterministically (canonical_cwd would pick this process's
        // cwd, which has no snapshots in the seeded ledger).
        snapshot.project_key = "/proj".to_string();
        let html = render_html(&snapshot, false);

        // The three tier labels are surfaced as headings or section labels.
        assert!(html.contains("Identified"), "Identified missing");
        assert!(html.contains("Realized"), "Realized missing");
        assert!(html.contains("Measured"), "Measured missing");
        // The three tier text labels (from tiers::LABEL_*) reach the document.
        assert!(html.contains("advisory estimate"), "tier 1 label missing");
        assert!(
            html.contains("measured structure, estimated frequency"),
            "tier 2 label missing"
        );
        // Tier 3 label only shows when measured is populated; this seed has
        // ingestion off, so we skip that assertion. Identified and realized
        // labels are required.

        // Honesty footer.
        assert!(
            html.contains("All savings are input-token bounded; output may vary."),
            "honesty footer missing"
        );

        // No script tag of any kind.
        assert!(!html.contains("<script"), "report must not embed scripts");

        // Minimal parseability: balanced <html> tags, doctype on top.
        assert!(html.starts_with("<!doctype html>"));
        assert_eq!(html.matches("<html").count(), 1);
        assert_eq!(html.matches("</html>").count(), 1);

        // Load profile populated since /proj has a project snapshot.
        assert!(
            html.contains("Load profile"),
            "load profile section missing"
        );

        // No em-dashes or en-dashes anywhere in the document.
        assert!(!html.contains('\u{2014}'), "em-dash leaked into output");
        assert!(!html.contains('\u{2013}'), "en-dash leaked into output");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_html_handles_empty_ledger_with_friendly_states() {
        // A consented ledger with no records still renders cleanly; every tier
        // falls back to its "empty" message rather than erroring.
        let dir = tmp("empty");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        let snapshot = StatsSnapshot::load_in(&dir);
        let html = render_html(&snapshot, true);
        assert!(html.contains("Tolkin savings report"));
        assert!(html.contains("No identified savings recorded yet."));
        assert!(html.contains("Two or more project snapshots"));
        assert!(html.contains("Usage-log ingestion is off."));
        assert!(html.contains("All savings are input-token bounded; output may vary."));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn html_escape_neutralizes_project_paths_with_quotes_and_brackets() {
        // The header surfaces the scope label; if the project path carries a
        // hostile string it must be escaped before it reaches the document.
        let dir = tmp("escape");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        let mut snapshot = StatsSnapshot::load_in(&dir);
        snapshot.project_key = "/tmp/\"<script>alert('x')</script>\"".to_string();
        let html = render_html(&snapshot, false);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
        let _ = fs::remove_dir_all(&dir);
    }
}
