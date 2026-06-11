use std::fs;
use std::io::IsTerminal;
use std::path::Path;

use anyhow::{bail, Result};
use clap::Args;
use serde_json::json;
use tolkin_core::pricing;

use crate::advisories::AdvisoryBlock;
use crate::ledger;
use crate::tiers::TierReport;
use crate::tui;

use super::stats_data::StatsSnapshot;

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Machine-wide stats across every project in the ledger.
    #[arg(long)]
    pub global: bool,

    /// Emit the full structured report as JSON.
    #[arg(long, conflicts_with_all = ["tui", "compact"])]
    pub json: bool,

    /// Delete the local ledger and parse cache (config.toml survives).
    #[arg(long, conflicts_with_all = ["tui", "compact"])]
    pub reset: bool,

    /// Open the dashboard (alternate screen, raw mode, interactive).
    #[arg(long, conflicts_with_all = ["json", "reset", "compact"])]
    pub tui: bool,

    /// Render one static frame to stdout (shareable artifact, no raw mode).
    #[arg(long, conflicts_with_all = ["json", "reset", "tui"])]
    pub compact: bool,
}

pub fn run(args: StatsArgs) -> Result<()> {
    if args.tui {
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            bail!("stats --tui requires a terminal (stdin and stdout must be a TTY)");
        }
        return tui::run();
    }
    if args.compact {
        let frame = tui::render_compact_frame()?;
        print!("{frame}");
        return Ok(());
    }

    let Some(dir) = ledger::data_dir() else {
        // No data dir yet: --json must still emit valid JSON.
        if args.json {
            let hints = vec!["run `tolkin init` to set up the local savings ledger"];
            let out = json!({
                "scope": if args.global { "global" } else { "project" },
                "project_key": serde_json::Value::Null,
                "generated_at": serde_json::Value::Null,
                "prices_observed": pricing::PRICES_OBSERVED,
                "realized_rate": serde_json::Value::Null,
                "ledger": { "records": 0, "skipped_lines": 0 },
                "ingestion": serde_json::Value::Null,
                "tiers": serde_json::Value::Null,
                "hints": hints,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
            return Ok(());
        }
        println!("No ledger yet.");
        println!("Hint: run `tolkin init` to set up the local savings ledger.");
        return Ok(());
    };
    if args.reset {
        return reset(&dir);
    }

    let snapshot = StatsSnapshot::load_in(&dir);
    if snapshot.records.is_empty() {
        // Empty ledger: --json must still emit valid JSON with zero/null values.
        if args.json {
            let mut hints =
                vec!["run `tolkin scan` or `tolkin project` in a repo to record a snapshot"];
            if ledger::disabled_by_env() {
                hints.push("writes are currently disabled (CI or TOLKIN_NO_LEDGER is set)");
            } else if snapshot.config.is_none() {
                hints.push("run `tolkin init` first to consent to the local ledger");
            }
            let out = json!({
                "scope": if args.global { "global" } else { "project" },
                "project_key": serde_json::Value::Null,
                "generated_at": snapshot.now,
                "prices_observed": pricing::PRICES_OBSERVED,
                "realized_rate": {
                    "usd_per_mtok_input": snapshot.rate_usd_per_mtok_input,
                    "model": snapshot.rate_model_id,
                },
                "ledger": { "records": 0, "skipped_lines": snapshot.skipped },
                "ingestion": {
                    "enabled": snapshot.ingestion_on,
                    "sessions_scanned": serde_json::Value::Null,
                    "parent_sessions": serde_json::Value::Null,
                    "subagent_streams": serde_json::Value::Null,
                    "skipped_lines": serde_json::Value::Null,
                    "skipped_files": serde_json::Value::Null,
                },
                "tiers": serde_json::Value::Null,
                "hints": hints,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
            return Ok(());
        }
        println!("No ledger records yet.");
        println!("Hint: run `tolkin scan` or `tolkin project` in a repo to record a snapshot.");
        if ledger::disabled_by_env() {
            println!("Note: writes are currently disabled (CI or TOLKIN_NO_LEDGER is set).");
        } else if snapshot.config.is_none() {
            println!("Hint: run `tolkin init` first to consent to the local ledger.");
        }
        return Ok(());
    }

    let scope_project = if args.global {
        None
    } else {
        Some(snapshot.project_key.as_str())
    };
    let report = if args.global {
        snapshot.compute_global()
    } else {
        snapshot.compute_project()
    };

    if args.json {
        // Additive blocks on the measured tier: `cache` (cache health) and
        // `advisories` (model mix, output share, cap runway). Strictly
        // additive; no existing key moves or renames (the skill schema lint
        // enforces the documented keys). Injected at the JSON layer so the
        // TierReport struct and its consumers stay untouched.
        let mut tiers_value = serde_json::to_value(&report)?;
        if !tiers_value["measured"].is_null() {
            if let Some(cache_report) = snapshot.compute_cache(scope_project) {
                tiers_value["measured"]["cache"] = serde_json::to_value(&cache_report)?;
            }
            if let Some(advisory_block) = snapshot.compute_advisories(&report) {
                tiers_value["measured"]["advisories"] = serde_json::to_value(&advisory_block)?;
            }
        }
        let out = json!({
            "scope": if args.global { "global" } else { "project" },
            "project_key": scope_project,
            "generated_at": snapshot.now,
            "prices_observed": pricing::PRICES_OBSERVED,
            "realized_rate": {
                "usd_per_mtok_input": snapshot.rate_usd_per_mtok_input,
                "model": snapshot.rate_model_id,
            },
            "ledger": { "records": snapshot.records.len(), "skipped_lines": snapshot.skipped },
            "ingestion": {
                "enabled": snapshot.ingestion_on,
                "sessions_scanned": snapshot.usage_data.as_ref().map(|d| d.sessions.len()),
                "parent_sessions": snapshot
                    .usage_data
                    .as_ref()
                    .map(|d| d.parent_session_count()),
                "subagent_streams": snapshot
                    .usage_data
                    .as_ref()
                    .map(|d| d.subagent_stream_count()),
                "skipped_lines": snapshot.usage_data.as_ref().map(|d| d.skipped_lines),
                "skipped_files": snapshot.usage_data.as_ref().map(|d| d.skipped_files),
            },
            "tiers": tiers_value,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let advisories = snapshot.compute_advisories(&report);
    let plain_args = PlainArgs {
        report: &report,
        advisories: advisories.as_ref(),
        scope_project,
        dir: &dir,
        record_count: snapshot.records.len(),
        skipped: snapshot.skipped,
        ingestion_on: snapshot.ingestion_on,
        rate_display: snapshot.rate_model_display,
        session_split: snapshot
            .usage_data
            .as_ref()
            .map(|d| (d.parent_session_count(), d.subagent_stream_count())),
    };
    print_plain(&plain_args);
    Ok(())
}

struct PlainArgs<'a> {
    report: &'a TierReport,
    advisories: Option<&'a AdvisoryBlock>,
    scope_project: Option<&'a str>,
    dir: &'a Path,
    record_count: usize,
    skipped: u64,
    ingestion_on: bool,
    rate_display: &'a str,
    // (parent sessions, subagent streams); the measured session count is the
    // sum and would read as a tripled headline without the split.
    session_split: Option<(usize, usize)>,
}

fn print_plain(args: &PlainArgs) {
    let PlainArgs {
        report,
        advisories,
        scope_project,
        dir,
        record_count,
        skipped,
        ingestion_on,
        rate_display,
        session_split,
    } = args;
    match scope_project {
        Some(p) => println!("Tolkin stats: {p}"),
        None => println!("Tolkin stats: all projects (machine-wide)"),
    }
    let skipped_note = if *skipped > 0 {
        format!(" ({skipped} unparseable skipped)")
    } else {
        String::new()
    };
    println!(
        "Ledger: {} records{skipped_note}  {}",
        record_count,
        ledger::ledger_path(dir).display()
    );
    println!();

    match &report.identified {
        Some(i) => {
            println!("Identified ({})", i.label);
            if let (Some(min), Some(max)) = (i.project_reclaimable_min, i.project_reclaimable_max) {
                println!(
                    "  project reclaimable: {}-{} tokens (as of {})",
                    commas(min),
                    commas(max),
                    i.project_as_of.map(format_utc).unwrap_or_default()
                );
            }
            if let (Some(swap), Some(slim)) = (i.mcp_swap_tokens, i.mcp_slim_tokens) {
                println!(
                    "  MCP swap: {} tokens, slim: {} tokens (as of {})",
                    commas(swap),
                    commas(slim),
                    i.mcp_as_of.map(format_utc).unwrap_or_default()
                );
            }
            if let (Some(min), Some(max)) = (i.audit_savings_min, i.audit_savings_max) {
                println!(
                    "  audit findings: {}-{} tokens (as of {})",
                    commas(min),
                    commas(max),
                    i.audit_as_of.map(format_utc).unwrap_or_default()
                );
            }
            if scope_project.is_none() {
                println!("  projects contributing: {}", i.projects);
            }
        }
        None => println!("Identified: nothing recorded yet (run tolkin project, scan, or audit)"),
    }
    println!();

    match &report.realized {
        Some(r) => {
            println!("Realized ({})", r.label);
            let sign = if r.tokens >= 0 { "" } else { "-" };
            println!(
                "  {sign}{} input tokens ({}{} at {} input rates) across {} sessions since {}",
                commas(r.tokens.unsigned_abs()),
                sign,
                usd(r.usd.abs()),
                rate_display,
                r.sessions_count,
                format_utc(r.since)
            );
            println!(
                "  always-loaded context: {} -> {} tokens ({} basis)",
                commas(r.baseline_always_tokens),
                commas(r.current_always_tokens),
                r.sessions_basis
            );
        }
        None => println!(
            "Realized: needs two or more project snapshots (re-run tolkin project after changes)"
        ),
    }
    println!();

    match &report.measured {
        Some(m) => {
            println!("Measured ({})", m.label);
            let span = match (m.first_ts, m.last_ts) {
                (Some(f), Some(l)) => format!("{} to {}", format_utc(f), format_utc(l)),
                _ => "no sessions in scope".to_string(),
            };
            match session_split {
                Some((parents, streams)) if *streams > 0 => println!(
                    "  {} sessions ({parents} working sessions + {streams} subagent streams), {span}",
                    m.sessions
                ),
                _ => println!("  {} sessions, {span}", m.sessions),
            }
            println!(
                "  input: {} fresh, {} cache read, {} cache write; output: {}",
                commas(m.totals.input_tokens),
                commas(m.totals.cache_read_tokens),
                commas(m.totals.cache_write_5m_tokens + m.totals.cache_write_1h_tokens),
                commas(m.totals.output_tokens)
            );
            println!(
                "  cache hit rate: {:.1}% of input-side tokens",
                m.cache_hit_rate * 100.0
            );
            println!(
                "  cost (priced models): {} (prices observed {})",
                usd(m.cost_usd_total),
                pricing::PRICES_OBSERVED
            );
            let mut models: Vec<_> = m.by_model.iter().collect();
            models.sort_by_key(|(_, mu)| std::cmp::Reverse(mu.totals.input_side()));
            for (model, mu) in models.iter().take(5) {
                let cost = match mu.cost_usd {
                    Some(c) => usd(c),
                    None => "unpriced".to_string(),
                };
                println!(
                    "    {model}: {} in / {} out, {cost}",
                    commas(mu.totals.input_side()),
                    commas(mu.totals.output_tokens)
                );
            }
        }
        None => {
            if *ingestion_on {
                println!("Measured: ingestion is on but no session logs were found");
            } else {
                println!("Measured: usage-log ingestion is off (re-run tolkin init to enable)");
            }
        }
    }
    println!();

    // Advisories block: model mix, output share, and cap runway.
    // Rendered only when measured data exists (same gate as the cache block).
    if let Some(block) = advisories {
        println!("Advisories ({})", block.label);
        // Model mix.
        let mix = &block.model_mix;
        if mix.priced_spend_total > 0.0 {
            if let Some(ref f) = mix.frontier {
                println!(
                    "  model mix: top model {} carries {:.1}% of priced spend ({})",
                    f.model,
                    f.share * 100.0,
                    usd(f.spend_usd)
                );
            }
            if let Some(ref c) = mix.cheap {
                println!(
                    "  model mix: cheapest-tier model {} carries {:.1}% of priced spend ({})",
                    c.model,
                    c.share * 100.0,
                    usd(c.spend_usd)
                );
            }
            if !mix.unpriced_models_excluded.is_empty() {
                println!(
                    "  model mix: unpriced models excluded from shares: {}",
                    mix.unpriced_models_excluded.join(", ")
                );
            }
            if let Some(ref adv) = mix.frontier_advisory {
                println!("  note: {adv}");
            }
            if let Some(ref adv) = mix.cheap_advisory {
                println!("  note: {adv}");
            }
        } else {
            println!("  model mix: no priced spend recorded");
        }
        // Output share.
        let os = &block.output_share;
        println!("  output share: {}", os.framing);
        // Cap runway (only when configured).
        if let Some(ref cap) = block.cap_runway {
            if cap.cap_already_reached {
                println!(
                    "  cap runway: cap ${:.2} already reached this month (MTD {})",
                    cap.monthly_cap_usd,
                    usd(cap.mtd_spend_usd)
                );
            } else {
                println!(
                    "  cap runway: MTD spend {}, remaining {} of cap ${}",
                    usd(cap.mtd_spend_usd),
                    usd(cap.remaining_usd),
                    cap.monthly_cap_usd
                );
                if let Some(ref date) = cap.rate_7d.projected_cap_date {
                    println!(
                        "    at your 7-day rate ({}/day) you reach ${:.2} on {}",
                        usd(cap.rate_7d.avg_daily_usd),
                        cap.monthly_cap_usd,
                        date
                    );
                } else if let Some(ref note) = cap.rate_7d.projection_note {
                    println!("    7-day projection: {note}");
                }
                if let Some(ref date) = cap.rate_30d.projected_cap_date {
                    println!(
                        "    at your 30-day rate ({}/day) you reach ${:.2} on {}",
                        usd(cap.rate_30d.avg_daily_usd),
                        cap.monthly_cap_usd,
                        date
                    );
                } else if let Some(ref note) = cap.rate_30d.projection_note {
                    println!("    30-day projection: {note}");
                }
            }
        }
        println!();
    }

    for note in &report.notes {
        println!("note: {note}");
    }
}

fn reset(dir: &Path) -> Result<()> {
    let path = ledger::ledger_path(dir);
    if path.is_file() {
        fs::remove_file(&path)?;
        println!("Deleted: {}", path.display());
    } else {
        println!("No ledger to delete at {}.", path.display());
    }
    let cache = dir.join("usage-cache.json");
    if cache.is_file() {
        fs::remove_file(&cache)?;
        println!("Deleted: {}", cache.display());
    }
    Ok(())
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

/// Format a unix epoch second count as "YYYY-MM-DD HH:MM UTC".
fn format_utc(secs: u64) -> String {
    let (y, mo, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    let h = rem / 3_600;
    let m = (rem % 3_600) / 60;
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

/// Howard Hinnant's civil_from_days algorithm: a closed-form conversion from
/// a count of days-from-Unix-epoch to a (year, month, day) Gregorian triple.
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
    fn format_utc_handles_known_epochs() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        // 2024-01-01 00:00:00 UTC = 1_704_067_200
        assert_eq!(format_utc(1_704_067_200), "2024-01-01 00:00 UTC");
        let s = format_utc(1_780_000_000);
        assert!(s.ends_with(" UTC"));
        assert!(s.starts_with("20"));
    }

    #[test]
    fn usd_and_commas_format_sanely() {
        assert_eq!(commas(1_234_567), "1,234,567");
        assert_eq!(usd(0.0), "$0");
        assert_eq!(usd(0.004), "$0.00400");
        assert_eq!(usd(0.5), "$0.500");
        assert_eq!(usd(12.345), "$12.35");
    }
}
