//! First-run preflight per IMPROVEMENTS W4. Plain prompts on stdout/stdin,
//! no TUI, no new dependencies. Runs once automatically when there is no
//! config and stdin/stdout are both TTYs; `tolkin init` re-runs it on demand.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::ledger::{self, Config};
use crate::scan;
use crate::tokenize::{self, Provider as TokProvider};

const CHECK_MARK: &str = "\u{2713}";

/// Entry point. `force=true` (from `tolkin init`) overwrites an existing
/// config. `yes=true` skips prompts and takes the defaults (ledger on,
/// ingestion off).
pub fn run(force: bool, yes: bool) -> Result<()> {
    if !force && ledger::load_config().is_some() {
        // Auto-trigger path: nothing to do if the user is already onboarded.
        return Ok(());
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "Welcome to tolkin.");
    let _ = writeln!(out, "Nothing leaves this machine.");
    let _ = writeln!(out);

    let _ = writeln!(out, "Preflight checks");
    check_tokenizers(&mut out);
    let path_var = std::env::var("PATH").unwrap_or_default();
    let agents = check_agent_clis(&mut out, &path_var);

    // Build scan roots so MCP and instruction probes target the right paths.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let roots = scan::ScanRoots::detect(cwd.clone()).unwrap_or_else(|_| scan::ScanRoots {
        home: home_dir(),
        cwd: cwd.clone(),
        config: None,
        data: None,
        os: scan::ScanOs::current(),
    });
    let mcp_total = check_mcp_configs(&mut out, &roots);
    let instr_total = check_instruction_files(&mut out, &roots, &cwd);
    let logs_detected = check_session_logs(&mut out, &roots.home);
    let _ = writeln!(out);

    // Consents.
    let consent_ledger;
    let consent_ingestion;
    let consent_update_check;
    if yes {
        consent_ledger = true;
        consent_ingestion = false;
        // Non-interactive: never silently consent to network checks.
        consent_update_check = None;
        let _ = writeln!(out, "Accepting defaults: ledger on, log ingestion off.");
    } else {
        let _ = writeln!(out);
        consent_ledger = prompt_yes_no(
            &mut out,
            "Keep a local savings ledger (nothing leaves this machine)?",
            true,
        );
        consent_ingestion = if logs_detected {
            prompt_yes_no(
                &mut out,
                "Ingest local agent session logs for real-usage stats (local only, off by default)?",
                false,
            )
        } else {
            false
        };
        let answered = prompt_yes_no(
            &mut out,
            "Allow tolkin to check for new versions once a day? This sends one plain HTTPS request to the npm registry and nothing else.",
            false,
        );
        consent_update_check = Some(answered);
    }

    let mut cfg = Config::new(consent_ledger, consent_ingestion);
    cfg.consent_update_check = consent_update_check;
    if let Err(e) = ledger::save_config(&cfg) {
        let _ = writeln!(out, "warn: could not save config: {e}");
    }

    let _ = writeln!(out);
    print_command_card(&mut out, mcp_total, instr_total, agents);
    Ok(())
}

fn check_tokenizers<W: Write>(out: &mut W) {
    let start = Instant::now();
    let mut all_ok = true;
    for p in TokProvider::ALL {
        if tokenize::count(p, "hello").is_err() {
            all_ok = false;
        }
    }
    let ms = start.elapsed().as_millis();
    let marker = if all_ok { CHECK_MARK } else { "--" };
    let _ = writeln!(out, "  {marker} tokenizers loaded ({ms} ms)");
}

/// Probe agent CLIs on PATH. Returns the list of found binaries so the
/// command card can suggest the most relevant next step.
fn check_agent_clis<W: Write>(out: &mut W, path_var: &str) -> Vec<&'static str> {
    const PROBES: [&str; 5] = ["claude", "codex", "gemini", "cursor", "code"];
    let found: Vec<&'static str> = PROBES
        .into_iter()
        .filter(|bin| scan::find_in_path(bin, path_var))
        .collect();
    let label = if found.is_empty() {
        "none on PATH".to_string()
    } else {
        found.join(", ")
    };
    let _ = writeln!(out, "  {CHECK_MARK} agent CLIs: {label}");
    found
}

fn check_mcp_configs<W: Write>(out: &mut W, roots: &scan::ScanRoots) -> usize {
    let sources = scan::existing_mcp_sources(roots);
    let mut by_client: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for (client, _) in &sources {
        *by_client.entry(client).or_insert(0) += 1;
    }
    if sources.is_empty() {
        let _ = writeln!(out, "  {CHECK_MARK} MCP configs: none found");
    } else {
        let parts: Vec<String> = by_client.iter().map(|(c, n)| format!("{c}: {n}")).collect();
        let _ = writeln!(
            out,
            "  {CHECK_MARK} MCP configs: {} ({})",
            sources.len(),
            parts.join(", ")
        );
    }
    sources.len()
}

fn check_instruction_files<W: Write>(out: &mut W, roots: &scan::ScanRoots, cwd: &Path) -> usize {
    let files = scan::existing_instruction_files(roots);
    let in_cwd = files.iter().filter(|p| p.starts_with(cwd)).count();
    let _ = writeln!(out, "  {CHECK_MARK} instruction files in cwd: {in_cwd}");
    in_cwd
}

fn check_session_logs<W: Write>(out: &mut W, home: &Path) -> bool {
    let claude = has_claude_sessions(home);
    let codex = has_codex_sessions(home);
    let mut parts = Vec::new();
    if claude {
        parts.push("Claude Code");
    }
    if codex {
        parts.push("Codex");
    }
    if parts.is_empty() {
        let _ = writeln!(out, "  {CHECK_MARK} agent session logs: none detected");
        false
    } else {
        let _ = writeln!(
            out,
            "  {CHECK_MARK} agent session logs: {}",
            parts.join(", ")
        );
        true
    }
}

fn has_claude_sessions(home: &Path) -> bool {
    let projects = home.join(".claude").join("projects");
    let Ok(entries) = fs::read_dir(&projects) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(inner) = fs::read_dir(&path) else {
            continue;
        };
        for f in inner.flatten() {
            if f.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
            {
                return true;
            }
        }
    }
    false
}

fn has_codex_sessions(home: &Path) -> bool {
    let sessions = home.join(".codex").join("sessions");
    let Ok(mut entries) = fs::read_dir(&sessions) else {
        return false;
    };
    entries.next().is_some()
}

fn prompt_yes_no<W: Write>(out: &mut W, prompt: &str, default_yes: bool) -> bool {
    let suffix = if default_yes { "[Y/n] " } else { "[y/N] " };
    let _ = write!(out, "{prompt} {suffix}");
    let _ = out.flush();
    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return default_yes;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default_yes,
    }
}

fn print_command_card<W: Write>(
    out: &mut W,
    mcp_total: usize,
    instr_total: usize,
    agents: Vec<&'static str>,
) {
    let _ = writeln!(out, "Try these next");
    let _ = writeln!(
        out,
        "  tolkin scan      discover local agent configs and instruction files"
    );
    let _ = writeln!(
        out,
        "  tolkin project   audit this repo's agent-context token footprint"
    );
    let _ = writeln!(out, "  tolkin stats     show the local savings ledger");

    let _ = writeln!(out);
    if mcp_total > 0 {
        let noun = if mcp_total == 1 { "config" } else { "configs" };
        let _ = writeln!(
            out,
            "Suggestion: {mcp_total} MCP {noun} found, start with tolkin scan."
        );
    } else if instr_total > 0 {
        let noun = if instr_total == 1 { "file" } else { "files" };
        let _ = writeln!(
            out,
            "Suggestion: {instr_total} instruction {noun} in this directory, start with tolkin project."
        );
    } else if agents.is_empty() {
        let _ = writeln!(
            out,
            "Suggestion: no agent CLIs detected on PATH; tolkin still works on any file or stdin."
        );
    } else {
        let _ = writeln!(
            out,
            "Suggestion: start with tolkin project in a repo of yours."
        );
    }
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}
