use std::env;
use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Args;
use serde_json::json;

use crate::ledger;
use crate::project::{self, FileEntry, ProjectOptions, ProjectReport};

#[derive(Args, Debug)]
pub struct ProjectArgs {
    /// Directory to audit. Defaults to the current directory.
    pub dir: Option<PathBuf>,

    /// Emit JSON instead of the human report.
    #[arg(long)]
    pub json: bool,

    /// Include experimental audit rules (higher false-positive risk).
    #[arg(long)]
    pub experimental: bool,

    /// How many entries the heaviest-files list shows.
    #[arg(long, default_value_t = 10)]
    pub top: usize,

    /// Exit with code 2 if any finding at or above this severity exists (CI hook).
    #[arg(long, value_parser = ["high", "medium", "low"])]
    pub fail_on: Option<String>,

    /// Skip files larger than this many bytes.
    #[arg(long, default_value_t = 2_000_000)]
    pub max_file_bytes: u64,
}

pub fn run(args: ProjectArgs) -> Result<()> {
    let dir = match args.dir {
        Some(d) => d,
        None => env::current_dir()?,
    };
    if !dir.is_dir() {
        bail!("{} is not a directory", dir.display());
    }
    let root = dir.canonicalize().unwrap_or(dir);
    let opts = ProjectOptions {
        experimental: args.experimental,
        max_file_bytes: args.max_file_bytes,
        top: args.top,
    };
    let report = project::analyze(&root, &opts);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&report, args.top));
    }

    // Headline numbers only (no file contents, no secret values). The
    // append() call gates itself on env and recorded consent; failures are
    // silent so a ledger problem cannot disrupt the command.
    let agent_context_files: u64 = report.totals.context_files as u64;
    ledger::append(
        "project",
        &root,
        json!({
            "files_scanned": report.totals.files_scanned,
            "agent_context_files": agent_context_files,
            "always_tokens": report.profiles.always.tokens,
            "on_invocation_tokens": report.profiles.on_invocation.tokens,
            "on_demand_tokens": report.profiles.on_demand.tokens,
            "docs_tokens": report.profiles.docs.tokens,
            "reclaimable_min": report.totals.savings_min,
            "reclaimable_max": report.totals.savings_max,
        }),
    );

    if let Some(threshold) = &args.fail_on {
        if project::exceeds_severity(&report, threshold) {
            // process::exit skips the normal end-of-main stdout teardown.
            use std::io::Write;
            let _ = std::io::stdout().flush();
            std::process::exit(2);
        }
    }
    Ok(())
}

fn render(r: &ProjectReport, top: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Tolkin project audit: {}", r.root);
    let _ = writeln!(out, "Nothing leaves this machine. Read-only.");
    let _ = writeln!(out);

    if r.totals.context_files == 0 && r.mcp_configs.is_empty() {
        let _ = writeln!(out, "no agent-context files found; is this the repo root?");
        let _ = writeln!(out);
        for w in &r.warnings {
            let _ = writeln!(out, "warn: {w}");
        }
        let _ = writeln!(
            out,
            "Total: {} files scanned, 0 in agent context",
            commas(r.totals.files_scanned)
        );
        return out;
    }

    let _ = writeln!(out, "Context weight by load profile");
    let rows = [
        ("always", &r.profiles.always, always_desc(r)),
        (
            "on-invocation",
            &r.profiles.on_invocation,
            invocation_desc(&r.profiles.on_invocation.files),
        ),
        (
            "on-demand",
            &r.profiles.on_demand,
            demand_desc(&r.profiles.on_demand.files),
        ),
        ("docs", &r.profiles.docs, docs_desc(&r.profiles.docs.files)),
    ];
    for (label, rollup, desc) in rows {
        let tokens = format!("~{} tokens", commas(rollup.tokens));
        let _ = writeln!(out, "  {label:<15} {tokens:<18} ({desc})");
    }
    let _ = writeln!(out);

    render_always_detail(&mut out, r);
    render_heaviest(&mut out, r, top);
    render_rules(&mut out, r);
    render_secrets(&mut out, r);

    for w in &r.warnings {
        let _ = writeln!(out, "warn: {w}");
    }
    if !r.warnings.is_empty() {
        let _ = writeln!(out);
    }

    let reclaimable = if r.totals.savings_max == 0 {
        "nothing obviously reclaimable".to_string()
    } else {
        format!(
            "{} reclaimable",
            range(r.totals.savings_min, r.totals.savings_max)
        )
    };
    let _ = writeln!(
        out,
        "Total: {} files scanned, {} in agent context (~{} tokens), {}",
        commas(r.totals.files_scanned),
        r.totals.context_files,
        commas(r.totals.context_tokens),
        reclaimable
    );
    let _ = writeln!(out, "Savings are input-token estimates; output may vary.");
    out
}

fn render_always_detail(out: &mut String, r: &ProjectReport) {
    use std::fmt::Write;
    let _ = writeln!(out, "Always-loaded detail");
    let mut rows: Vec<(String, String)> = Vec::new();
    for f in &r.profiles.always.files {
        if f.class == "skill-frontmatter" {
            continue;
        }
        rows.push((f.path.clone(), format!("{} tokens", commas(f.tokens))));
    }
    let skills: Vec<&FileEntry> = r
        .profiles
        .always
        .files
        .iter()
        .filter(|f| f.class == "skill-frontmatter")
        .collect();
    if !skills.is_empty() {
        let total: u64 = skills.iter().map(|f| f.tokens).sum();
        rows.push((
            format!("skill descriptions ({})", plural(skills.len(), "skill")),
            format!("{} tokens", commas(total)),
        ));
    }
    for m in &r.mcp_configs {
        rows.push((
            format!("{} ({})", m.path, plural(m.servers, "server")),
            format!(
                "{} cold tokens   (run tolkin mcp for detail)",
                commas(m.cold_tokens)
            ),
        ));
    }
    if rows.is_empty() {
        let _ = writeln!(out, "  none found");
    }
    let width = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    for (label, value) in rows {
        let _ = writeln!(out, "  {label:<width$}  {value}");
    }
    let _ = writeln!(out);
}

fn render_heaviest(out: &mut String, r: &ProjectReport, top: usize) {
    use std::fmt::Write;
    if r.heaviest.is_empty() {
        return;
    }
    let _ = writeln!(out, "Heaviest files (top {top})");
    let width = r
        .heaviest
        .iter()
        .map(|h| h.path.chars().count())
        .max()
        .unwrap_or(0);
    for h in &r.heaviest {
        let suffix = if h.findings > 0 {
            format!(
                "   {} ({} reclaimable)",
                plural(h.findings, "finding"),
                range(h.savings_min, h.savings_max)
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  {:<width$}  {:>8} tokens{suffix}",
            h.path,
            commas(h.tokens)
        );
    }
    let _ = writeln!(out);
}

fn render_rules(out: &mut String, r: &ProjectReport) {
    use std::fmt::Write;
    let _ = writeln!(out, "Findings by rule");
    if r.findings_by_rule.is_empty() {
        let _ = writeln!(out, "  none; agent context is lean");
    }
    let width = r
        .findings_by_rule
        .iter()
        .map(|f| f.rule.chars().count())
        .max()
        .unwrap_or(0);
    for f in &r.findings_by_rule {
        let savings = if f.savings_min == f.savings_max {
            format!("{} tokens (exact)", range(f.savings_min, f.savings_max))
        } else {
            format!("{} tokens", range(f.savings_min, f.savings_max))
        };
        let _ = writeln!(
            out,
            "  {:<width$}  {}   {savings}",
            f.rule,
            plural(f.count, "finding")
        );
    }
    let _ = writeln!(out);
}

fn render_secrets(out: &mut String, r: &ProjectReport) {
    use std::fmt::Write;
    if r.secret_files.is_empty() {
        return;
    }
    let _ = writeln!(out, "Secrets");
    for s in &r.secret_files {
        let noun = if s.secret_count == 1 {
            "likely secret"
        } else {
            "likely secrets"
        };
        let _ = writeln!(
            out,
            "  {}: {} {noun} ({})",
            s.path,
            s.secret_count,
            s.kinds.join(", ")
        );
    }
    let _ = writeln!(
        out,
        "  Remove them; agent-context files are sent to the model."
    );
    let _ = writeln!(out);
}

/// Parenthetical for the always row, e.g.
/// "CLAUDE.md x2, AGENTS.md, 14 skill descriptions, 2 MCP configs".
fn always_desc(r: &ProjectReport) -> String {
    let files = &r.profiles.always.files;
    let mut parts: Vec<String> = Vec::new();
    for (class, label) in [
        ("claude-md", "CLAUDE.md"),
        ("agents-md", "AGENTS.md"),
        ("gemini-md", "GEMINI.md"),
        ("cursorrules", ".cursorrules"),
        ("windsurfrules", ".windsurfrules"),
        ("copilot-instructions", "copilot-instructions.md"),
    ] {
        match count_class(files, class) {
            0 => {}
            1 => parts.push(label.to_string()),
            n => parts.push(format!("{label} x{n}")),
        }
    }
    push_count(&mut parts, count_class(files, "cursor-rule"), "cursor rule");
    push_count(
        &mut parts,
        count_class(files, "skill-frontmatter"),
        "skill description",
    );
    push_count(&mut parts, r.mcp_configs.len(), "MCP config");
    join_or_none(parts)
}

fn invocation_desc(files: &[FileEntry]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let bodies = count_class(files, "skill-body");
    if bodies == 1 {
        parts.push("1 skill body".to_string());
    } else if bodies > 1 {
        parts.push(format!("{bodies} skill bodies"));
    }
    push_count(&mut parts, count_class(files, "command"), "command");
    push_count(&mut parts, count_class(files, "agent"), "agent");
    push_count(
        &mut parts,
        count_class(files, "codex-prompt"),
        "codex prompt",
    );
    join_or_none(parts)
}

fn demand_desc(files: &[FileEntry]) -> String {
    if files.is_empty() {
        "none found".to_string()
    } else {
        plural(files.len(), "reference file")
    }
}

fn docs_desc(files: &[FileEntry]) -> String {
    if files.is_empty() {
        "none found".to_string()
    } else {
        plural(files.len(), "root markdown file")
    }
}

fn count_class(files: &[FileEntry], class: &str) -> usize {
    files.iter().filter(|f| f.class == class).count()
}

fn push_count(parts: &mut Vec<String>, n: usize, noun: &str) {
    if n > 0 {
        parts.push(plural(n, noun));
    }
}

fn join_or_none(parts: Vec<String>) -> String {
    if parts.is_empty() {
        "none found".to_string()
    } else {
        parts.join(", ")
    }
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn range(min: u64, max: u64) -> String {
    if min == max {
        format!("~{}", commas(min))
    } else {
        format!("~{}-{}", commas(min), commas(max))
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
    use crate::project::{
        FindingSummary, HeavyFile, McpConfigEntry, Profile, ProfileRollup, Profiles, RuleRollup,
        SecretFile, Totals,
    };

    fn file(path: &str, class: &str, profile: Profile, tokens: u64) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            class: class.to_string(),
            profile,
            tokens,
            findings: Vec::new(),
            secret_count: 0,
            secret_kinds: Vec::new(),
            skill_name: None,
            skill_description: None,
        }
    }

    fn empty_report() -> ProjectReport {
        ProjectReport {
            root: "/repo".to_string(),
            profiles: Profiles::default(),
            mcp_configs: Vec::new(),
            heaviest: Vec::new(),
            findings_by_rule: Vec::new(),
            secret_files: Vec::new(),
            totals: Totals {
                files_scanned: 12,
                context_files: 0,
                context_tokens: 0,
                savings_min: 0,
                savings_max: 0,
            },
            warnings: Vec::new(),
        }
    }

    fn populated_report() -> ProjectReport {
        let mut claude = file("CLAUDE.md", "claude-md", Profile::Always, 3_816);
        claude.findings = vec![FindingSummary {
            rule: "near-duplicate-paragraphs".to_string(),
            severity: "medium".to_string(),
            savings_min: 480,
            savings_max: 1_100,
        }];
        let fm = FileEntry {
            skill_name: Some("deploy".to_string()),
            ..file(
                ".claude/skills/deploy/SKILL.md",
                "skill-frontmatter",
                Profile::Always,
                86,
            )
        };
        let body = file(
            ".claude/skills/deploy/SKILL.md",
            "skill-body",
            Profile::OnInvocation,
            6_127,
        );
        ProjectReport {
            root: "/repo".to_string(),
            profiles: Profiles {
                always: ProfileRollup {
                    tokens: 3_816 + 86 + 4_910,
                    files: vec![claude, fm],
                },
                on_invocation: ProfileRollup {
                    tokens: 6_127,
                    files: vec![body],
                },
                on_demand: ProfileRollup::default(),
                docs: ProfileRollup {
                    tokens: 50,
                    files: vec![file("README.md", "doc", Profile::Docs, 50)],
                },
            },
            mcp_configs: vec![McpConfigEntry {
                path: ".mcp.json".to_string(),
                servers: 5,
                cold_tokens: 4_910,
            }],
            heaviest: vec![HeavyFile {
                path: ".claude/skills/deploy/SKILL.md".to_string(),
                tokens: 6_213,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            }],
            findings_by_rule: vec![RuleRollup {
                rule: "near-duplicate-paragraphs".to_string(),
                count: 1,
                savings_min: 480,
                savings_max: 1_100,
            }],
            secret_files: vec![SecretFile {
                path: ".claude/skills/db/SKILL.md".to_string(),
                secret_count: 1,
                kinds: vec!["db-uri".to_string()],
            }],
            totals: Totals {
                files_scanned: 214,
                context_files: 4,
                context_tokens: 14_989,
                savings_min: 480,
                savings_max: 1_100,
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn empty_report_renders_calm_state() {
        let out = render(&empty_report(), 10);
        assert!(out.contains("Nothing leaves this machine. Read-only."));
        assert!(out.contains("no agent-context files found; is this the repo root?"));
        assert!(out.contains("Total: 12 files scanned, 0 in agent context"));
    }

    #[test]
    fn populated_report_renders_every_section() {
        let out = render(&populated_report(), 10);
        assert!(out.contains("Context weight by load profile"));
        assert!(out.contains("CLAUDE.md, 1 skill description, 1 MCP config"));
        assert!(out.contains("1 skill body"));
        assert!(out.contains("skill descriptions (1 skill)"));
        assert!(out.contains(".mcp.json (5 servers)"));
        assert!(out.contains("4,910 cold tokens   (run tolkin mcp for detail)"));
        assert!(out.contains("Heaviest files (top 10)"));
        assert!(out.contains("near-duplicate-paragraphs"));
        assert!(out.contains("~480-1,100 tokens"));
        assert!(out.contains(".claude/skills/db/SKILL.md: 1 likely secret (db-uri)"));
        assert!(out.contains(
            "Total: 214 files scanned, 4 in agent context (~14,989 tokens), ~480-1,100 reclaimable"
        ));
        assert!(out.contains("Savings are input-token estimates; output may vary."));
    }
}
