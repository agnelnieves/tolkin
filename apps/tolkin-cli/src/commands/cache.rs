//! `tolkin cache`: the measured prompt-cache health report.
//!
//! Surfaces the analysis in `crate::cache_analysis` over the per-request
//! tuples retained by usage-log ingestion: hit rate with the broken-cache
//! advisory, write churn, the 5m-vs-1h TTL counterfactual, and cadence
//! facts. Consent behavior mirrors `tolkin stats`: no data dir or no
//! ingestion consent prints the same style of friendly hints and exits 0.
//! An ingestion-on scope with zero sessions is a real (empty) report, so
//! `--json` emits the stable schema with zeros there instead of prose.
//!
//! Privacy posture: the report contains session ids, timestamps, and token
//! counts only; never message content, and no project paths beyond the
//! scope key the ledger already stores.

use anyhow::Result;
use clap::Args;
use serde_json::json;
use tolkin_core::pricing;

use crate::cache_analysis::CacheReport;
use crate::ledger;

use super::stats_data::StatsSnapshot;

#[derive(Args, Debug)]
pub struct CacheArgs {
    /// Machine-wide cache health across every project in the logs.
    #[arg(long)]
    pub global: bool,

    /// Emit the full structured report as JSON.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: CacheArgs) -> Result<()> {
    let Some(dir) = ledger::data_dir() else {
        // Mirrors stats: a missing data dir is a setup hint, not an error.
        println!("No ledger yet.");
        println!("Hint: run `tolkin init` to set up the local savings ledger.");
        return Ok(());
    };
    let snapshot = StatsSnapshot::load_in(&dir);

    let scope_project = if args.global {
        None
    } else {
        Some(snapshot.project_key.as_str())
    };
    let Some(report) = snapshot.compute_cache(scope_project) else {
        // Ingestion off: same consent posture as the stats measured tier.
        println!("Cache analysis needs usage-log ingestion, which is off.");
        println!("Hint: re-run `tolkin init` and consent to log ingestion.");
        if ledger::disabled_by_env() {
            println!("Note: ingestion is currently disabled (CI or TOLKIN_NO_LEDGER is set).");
        }
        return Ok(());
    };

    if args.json {
        let out = json!({
            "scope": if args.global { "global" } else { "project" },
            "project_key": scope_project,
            "generated_at": snapshot.now,
            "prices_observed": pricing::PRICES_OBSERVED,
            "cache": report,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    print_plain(&report, scope_project);
    Ok(())
}

fn print_plain(report: &CacheReport, scope_project: Option<&str>) {
    match scope_project {
        Some(p) => println!("Tolkin cache health: {p}"),
        None => println!("Tolkin cache health: all projects (machine-wide)"),
    }
    println!(
        "Source: Claude Code session logs ({} sessions, {} requests). Codex rollouts carry no cache-write fields.",
        commas(report.sessions_analyzed),
        commas(report.requests_analyzed)
    );
    println!();

    let hr = &report.hit_rate;
    println!("Hit rate ({})", hr.label);
    println!(
        "  {:.1}% cache reads over input-side tokens ({} / {})",
        hr.rate * 100.0,
        commas(hr.cache_read_tokens),
        commas(hr.input_side_tokens)
    );
    match &hr.advisory {
        Some(advisory) => {
            println!("  advisory: {advisory}");
            for d in &hr.days_below_threshold {
                println!(
                    "    {}: {:.1}% over {} input-side tokens",
                    d.day,
                    d.rate * 100.0,
                    commas(d.input_side_tokens)
                );
            }
        }
        None => println!(
            "  no active day under the {:.2} broken-cache threshold",
            hr.threshold
        ),
    }
    println!();

    let churn = &report.write_churn;
    println!("Write churn ({})", churn.label);
    // Headline copy is deliberately a description of what the share counts
    // (writes after the first within a session), not a flat claim that the
    // prefix changed. The CHURN_NOTE below explains why a high share on a
    // Claude Code transcript reflects conversation growth as well as
    // instability, so the headline must not foreclose that reading.
    println!(
        "  {:.1}% of cache-write tokens were written after a session's first write ({} of {})",
        churn.share * 100.0,
        commas(churn.writes_after_first_tokens),
        commas(churn.total_write_tokens)
    );
    println!(
        "  sessions with cache writes: {}",
        commas(churn.sessions_with_writes)
    );
    println!("  {}", crate::cache_analysis::CHURN_NOTE);
    if churn.worst_sessions.is_empty() {
        println!("  no session wrote after its first write");
    } else {
        println!("  worst sessions (tokens written after the first write):");
        for w in &churn.worst_sessions {
            println!(
                "    {} started {}: {} of {} write tokens after first ({:.1}%)",
                w.session_id,
                format_utc(w.first_ts),
                commas(w.writes_after_first_tokens),
                commas(w.write_tokens),
                w.share * 100.0
            );
        }
    }
    println!();

    let ttl = &report.ttl_counterfactual;
    println!(
        "TTL counterfactual ({}, from {} inputs)",
        ttl.label, ttl.inputs_label
    );
    println!(
        "  observed cache writes ({}): {} tokens at the 5m TTL, {} at the 1h TTL",
        ttl.inputs_label,
        commas(ttl.observed_write_tokens_5m),
        commas(ttl.observed_write_tokens_1h)
    );
    println!(
        "  simulated stable-prefix re-writes over the real gap timeline (reads refresh the TTL); {}",
        ttl.prefix_proxy
    );
    println!(
        "    5m strategy: W5 = {} write events, {} write tokens ({})",
        commas(ttl.simulated_w5_write_events),
        commas(ttl.simulated_w5_write_tokens),
        ttl.label
    );
    println!(
        "    1h strategy: W1 = {} write events, {} write tokens ({})",
        commas(ttl.simulated_w1_write_events),
        commas(ttl.simulated_w1_write_tokens),
        ttl.label
    );
    if ttl.sessions_without_writes_excluded > 0 {
        println!(
            "    {} session(s) with no observed cache write excluded (no prefix to simulate)",
            commas(ttl.sessions_without_writes_excluded)
        );
    }
    println!(
        "  marginal write cost: a 5m write bills 1.25x input but replaces a 0.1x cached read (marginal {:.2}x); a 1h write bills 2x minus the same 0.1x (marginal {:.1}x)",
        ttl.marginal_multiplier_5m, ttl.marginal_multiplier_1h
    );
    println!("  break even: {}", ttl.break_even);
    println!(
        "  dollars at per-model rates (prices observed {}): 5m strategy {}, 1h strategy {}, delta (1h minus 5m) {} ({})",
        pricing::PRICES_OBSERVED,
        usd(ttl.usd_5m_strategy),
        usd(ttl.usd_1h_strategy),
        usd_signed(ttl.usd_delta_1h_minus_5m),
        ttl.label
    );
    if !ttl.unpriced_models.is_empty() {
        println!(
            "  note: dollar figures cover priced models only; {:.1}% of simulated write tokens are priced (unpriced: {})",
            ttl.priced_share_of_simulated_tokens * 100.0,
            ttl.unpriced_models.join(", ")
        );
    }
    println!("  verdict: {}", ttl.verdict);
    println!("  {}", ttl.tier_note);
    println!();

    let cadence = &report.cadence;
    println!("Cadence ({})", cadence.label);
    println!(
        "  {:.1}% of intra-session gaps exceed 5 minutes ({} of {})",
        cadence.share_intra_gaps_over_5m * 100.0,
        commas(cadence.intra_session_gaps_over_5m),
        commas(cadence.intra_session_gaps)
    );
    println!(
        "  {:.1}% of inter-session gaps within a project are under an hour ({} of {}); that is how often a 1h cache could survive between sessions",
        cadence.share_inter_gaps_under_1h * 100.0,
        commas(cadence.inter_session_gaps_under_1h),
        commas(cadence.inter_session_gaps)
    );
    println!(
        "  sessions with zero cache reads: {}",
        commas(cadence.sessions_zero_cache_read)
    );
    println!();

    println!("{}", report.scope_line);
    for note in &report.notes {
        println!("note: {note}");
    }
}

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

/// Dollar formatting, mirroring `commands::stats::usd` so cache numbers
/// match the stats surface exactly.
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

fn usd_signed(v: f64) -> String {
    if v < 0.0 {
        format!("-{}", usd(-v))
    } else {
        usd(v)
    }
}

/// "YYYY-MM-DD HH:MM UTC", mirrored from `commands::stats`.
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

    #[test]
    fn usd_signed_carries_the_sign() {
        assert_eq!(usd_signed(0.0), "$0");
        assert_eq!(usd_signed(12.345), "$12.35");
        assert_eq!(usd_signed(-12.345), "-$12.35");
        assert_eq!(usd_signed(-0.004), "-$0.00400");
    }
}
