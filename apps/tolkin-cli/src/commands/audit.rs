use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use serde_json::json;
use tolkin_core::audit::{self, AuditOptions, AuditReport};

use crate::input;
use crate::ledger;
use crate::tokenize::{self, Provider};

/// Audit one file and return the engine report. No printing, no filtering,
/// no ledger write: the TUI's audit worker consumes this seam off the main
/// thread; the CLI command path below keeps its own flow.
pub fn audit_file(path: &Path) -> Result<AuditReport> {
    let text = input::read(Some(path))?;
    let input_tokens = tokenize::count(Provider::OpenAi, &text)? as u64;
    Ok(audit::audit(
        &text,
        &AuditOptions {
            input_tokens: Some(input_tokens),
            include_experimental: false,
        },
    ))
}

#[derive(Args, Debug)]
pub struct AuditArgs {
    /// File to audit, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Only show findings at or above this severity.
    #[arg(long, value_parser = ["high", "medium", "low"])]
    pub severity: Option<String>,

    /// Only show findings for one rule id (e.g. json-verbosity).
    #[arg(long, value_name = "ID")]
    pub rule: Option<String>,

    /// Include experimental rules (higher false-positive risk; findings are
    /// tagged [EXP]).
    #[arg(long)]
    pub experimental: bool,

    /// Emit the AuditReport as JSON instead of the ranked list.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: AuditArgs) -> Result<()> {
    let target = args
        .file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "stdin".to_string());
    let text = input::read(args.file.as_deref())?;
    // The CLI tokenizes natively, so the engine gets a real count (o200k_base
    // as the reference) instead of falling back to bytes/4.
    let input_tokens = tokenize::count(Provider::OpenAi, &text)? as u64;
    let mut report = audit::audit(
        &text,
        &AuditOptions {
            input_tokens: Some(input_tokens),
            include_experimental: args.experimental,
        },
    );

    if let Some(min) = &args.severity {
        let bar = rank(min);
        report.findings.retain(|f| rank(&f.severity) <= bar);
    }
    if let Some(rule) = &args.rule {
        report.findings.retain(|f| &f.rule == rule);
    }
    // Keep the totals consistent with whatever the filters left visible.
    report.total_savings_min = report.findings.iter().map(|f| f.savings_min).sum();
    report.total_savings_max = report.findings.iter().map(|f| f.savings_max).sum();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report, input_tokens);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    ledger::append(
        "audit",
        &cwd,
        json!({
            "input_tokens": input_tokens,
            "findings": report.findings.len() as u64,
            "savings_min": report.total_savings_min,
            "savings_max": report.total_savings_max,
            "target": target,
        }),
    );
    Ok(())
}

fn rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn print_report(report: &AuditReport, input_tokens: u64) {
    if report.findings.is_empty() {
        println!("No findings.");
    }
    for f in &report.findings {
        let exp = if f.badge == "experimental" {
            " [EXP]"
        } else {
            ""
        };
        println!(
            "[{}]{exp} {}  saves ~{}-{} tokens",
            f.severity.to_uppercase(),
            f.rule,
            commas(f.savings_min),
            commas(f.savings_max)
        );
        println!("    {}", f.title);
        println!("    {}", f.detail);
        println!("    {}", f.citation);
        if let Some(p) = &f.preview {
            print_preview(p);
        }
        println!();
    }

    println!(
        "Total potential savings: {}-{} of {} input tokens.",
        commas(report.total_savings_min),
        commas(report.total_savings_max),
        commas(input_tokens)
    );
    println!("Notes:");
    for n in &report.notes {
        println!("  - {n}");
    }
}

/// Render a finding's format preview indented under it, capped at 10 lines so
/// big conversions do not swamp the terminal.
fn print_preview(p: &tolkin_core::format::FormatPreview) {
    const MAX_PREVIEW_LINES: usize = 10;
    println!(
        "    preview ({}, {}): {} -> {} bytes",
        p.kind, p.fidelity, p.bytes_before, p.bytes_after
    );
    if !p.caveat.is_empty() {
        println!("    caveat: {}", p.caveat);
    }
    let lines: Vec<&str> = p.preview.lines().collect();
    for line in lines.iter().take(MAX_PREVIEW_LINES) {
        println!("      {line}");
    }
    if lines.len() > MAX_PREVIEW_LINES {
        println!(
            "      ... ({} more lines, use --json for full preview)",
            lines.len() - MAX_PREVIEW_LINES
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn audit_file_returns_a_report_for_a_real_file_and_errors_on_missing() {
        let dir = std::env::temp_dir().join(format!(
            "tolkin-audit-file-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("context.md");
        let para = "This exact paragraph repeats verbatim several times so the \
                    near-duplicate rule has real material to consider during the audit.";
        fs::write(&path, format!("{para}\n\n{para}\n\n{para}\n")).unwrap();

        let report = audit_file(&path).expect("audit runs on a real file");
        assert!(report.total_savings_min <= report.total_savings_max);
        for f in &report.findings {
            assert!(f.savings_min <= f.savings_max);
        }

        assert!(
            audit_file(&dir.join("missing.md")).is_err(),
            "missing files error cleanly instead of panicking"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
