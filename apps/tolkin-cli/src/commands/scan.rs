use std::env;
use std::path::Path;

use anyhow::Result;
use clap::Args;
use serde_json::{json, Value};
use tolkin_core::Provider as CoreProvider;

use crate::ledger;
use crate::scan::{self, ScanReport, ScanRoots};
use crate::tokenize::Provider as TokProvider;

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Provider for the MCP cache and dollar math (anthropic carries the cache surcharge).
    #[arg(long, default_value = "anthropic")]
    pub provider: TokProvider,

    /// Also run the full audit engine on each discovered instruction file.
    #[arg(long)]
    pub deep: bool,

    /// Emit JSON instead of the human report.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: ScanArgs) -> Result<()> {
    let cwd = env::current_dir()?;
    let roots = ScanRoots::detect(cwd)?;
    let path_var = env::var("PATH").unwrap_or_default();
    let report = scan::scan(&roots, &path_var, core_provider(args.provider), args.deep);

    let home = roots.home.clone();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&to_json(&report, args.provider))?
        );
    } else {
        print!("{}", render(&report, &home, args.provider, args.deep));
    }

    ledger::append(
        "scan",
        &roots.cwd,
        json!({
            "configs_found": report.mcp.len() as u64,
            "cold_tokens_total": report.mcp.iter().map(|m| m.analysis.totals.cold).sum::<u64>(),
            "swap_savings_tokens": report.mcp.iter().map(|m| m.analysis.totals.savings_tokens).sum::<u64>(),
            "slim_savings_tokens": report.mcp.iter().map(|m| m.analysis.totals.slim_savings_tokens).sum::<u64>(),
        }),
    );
    Ok(())
}

fn core_provider(p: TokProvider) -> CoreProvider {
    match p {
        TokProvider::OpenAi => CoreProvider::OpenAi,
        TokProvider::Anthropic => CoreProvider::Anthropic,
        TokProvider::Gemini => CoreProvider::Gemini,
    }
}

fn render(r: &ScanReport, home: &Path, provider: TokProvider, deep: bool) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Tolkin scan: local agent configuration audit");
    let _ = writeln!(out, "Nothing leaves this machine. Read-only scan.");
    let _ = writeln!(out);

    let _ = writeln!(out, "MCP configs ({} found)", r.mcp.len());
    if r.mcp.is_empty() {
        let _ = writeln!(
            out,
            "  no MCP servers found in the usual client config locations"
        );
    }
    for m in &r.mcp {
        let _ = writeln!(out, "  {}  {}", m.client, scan::display_path(&m.path, home));
        let t = &m.analysis.totals;
        let unknown = if t.unknown > 0 {
            format!(" ({} unrecognized, excluded from totals)", t.unknown)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "    {} servers{unknown}, ~{} cold tokens, {}% of a 200K window",
            t.servers,
            commas(t.cold),
            t.pct_of_window
        );
        for s in &m.swaps {
            let annotation = if s.installed {
                format!("({} is installed, zero-setup swap)", s.binary)
            } else {
                format!("(requires: {})", s.install_hint)
            };
            let _ = writeln!(
                out,
                "    swap {} -> {} {annotation}: save ~{} tokens/session",
                s.server,
                s.cli,
                commas(s.savings_tokens)
            );
        }
        let slimmable = m
            .analysis
            .servers
            .iter()
            .filter(|s| {
                s.slim
                    .as_ref()
                    .is_some_and(|x| !x.already_slimmed && x.est_tokens_saved > 0)
            })
            .count();
        if slimmable > 0 {
            let noun = if slimmable == 1 { "server" } else { "servers" };
            let _ = writeln!(
                out,
                "    {slimmable} {noun} can be slimmed for ~{} tokens; run tolkin mcp {} for snippets",
                commas(m.analysis.totals.slim_savings_tokens),
                scan::display_path(&m.path, home)
            );
        }
    }
    let _ = writeln!(out);

    let total_tokens: u64 = r.instruction_files.iter().map(|f| f.tokens).sum();
    let _ = writeln!(
        out,
        "Instruction files ({} found, ~{} tokens total)",
        r.instruction_files.len(),
        commas(total_tokens)
    );
    if r.instruction_files.is_empty() {
        let _ = writeln!(out, "  no agent instruction files detected");
    } else {
        let paths: Vec<String> = r
            .instruction_files
            .iter()
            .map(|f| scan::display_path(&f.path, home))
            .collect();
        let width = paths.iter().map(|p| p.chars().count()).max().unwrap_or(0);
        for (f, p) in r.instruction_files.iter().zip(&paths) {
            let _ = writeln!(
                out,
                "  {p:<width$}  {:>8} tokens",
                commas(f.tokens),
                width = width
            );
            if let Some(report) = &f.audit {
                if report.findings.is_empty() {
                    let _ = writeln!(out, "    audit: clean");
                }
                for finding in &report.findings {
                    let _ = writeln!(
                        out,
                        "    [{}] {}  saves ~{}-{} tokens",
                        finding.severity.to_uppercase(),
                        finding.rule,
                        commas(finding.savings_min),
                        commas(finding.savings_max)
                    );
                }
            }
        }
        let _ = writeln!(
            out,
            "  note: resent on every session start; cached reads are ~90% cheaper on Anthropic"
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "Shell configs");
    if r.shell.is_empty() {
        let _ = writeln!(out, "  none found");
    } else {
        let flagged = r.shell.iter().filter(|s| s.secret_count > 0).count();
        for s in r.shell.iter().filter(|s| s.secret_count > 0) {
            let noun = if s.secret_count == 1 {
                "likely secret"
            } else {
                "likely secrets"
            };
            let _ = writeln!(
                out,
                "  {}: {} {noun} exported ({})",
                scan::display_path(&s.path, home),
                s.secret_count,
                s.kinds.join(", ")
            );
        }
        if flagged == 0 {
            let _ = writeln!(
                out,
                "  no likely secrets found in {} shell config(s)",
                r.shell.len()
            );
        } else {
            let _ = writeln!(
                out,
                "  Keys in shell exports reach every agent subprocess; prefer a secret manager or per-tool config."
            );
        }
    }
    let _ = writeln!(out);

    // Workflow LLM prompts: per-CI-run, reported in a separate bucket.
    if !r.workflows_llm.is_empty() {
        let total_prompt_tokens: u64 = r.workflows_llm.iter().map(|w| w.prompt_tokens).sum();
        let _ = writeln!(
            out,
            "Workflow LLM prompts ({} file(s), ~{} tokens of prompt-bearing fields)",
            r.workflows_llm.len(),
            commas(total_prompt_tokens)
        );
        for w in &r.workflows_llm {
            let _ = writeln!(
                out,
                "  {}  ~{} prompt tokens",
                scan::display_path(&w.path, home),
                commas(w.prompt_tokens)
            );
            for f in &w.prompt_fields {
                let _ = writeln!(out, "    {}: ~{} tokens", f.field, commas(f.tokens));
            }
        }
        let _ = writeln!(
            out,
            "  note: workflow prompts load per-CI-run, not per-agent-session; these tokens are not in the always-loaded total above"
        );
        let _ = writeln!(out);
    }

    if r.environment.is_empty() {
        let _ = writeln!(out, "Environment: no relevant binaries detected on PATH");
    } else {
        let _ = writeln!(out, "Environment: {} detected", r.environment.join(", "));
    }

    // Hooks section
    if r.hooks.is_empty() {
        let _ = writeln!(out, "Hooks: none configured");
        let _ = writeln!(
            out,
            "  No hooks key found in .claude/settings.json or .claude/settings.local.json."
        );
        let _ = writeln!(
            out,
            "  Paste one or both of the following into .claude/settings.json to guard large"
        );
        let _ = writeln!(out, "  tool calls and cap oversized outputs (hooks docs: https://code.claude.com/docs/en/hooks):");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  PreToolUse guard (block Bash commands whose stdin exceeds 10 KB):"
        );
        let _ = writeln!(
            out,
            r#"  {{
    "hooks": {{
      "PreToolUse": [
        {{
          "matcher": "Bash",
          "hooks": [
            {{
              "type": "command",
              "command": "input=$(cat); size=$(printf '%s' \"$input\" | wc -c); if [ \"$size\" -gt 10240 ]; then echo \"{{\\\"decision\\\": \\\"block\\\", \\\"reason\\\": \\\"stdin exceeds 10 KB (\" + $size + \" bytes); trim the input before running\\\"}}\"; else echo \"$input\"; fi"
            }}
          ]
        }}
      ]
    }}
  }}"#
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  PostToolUse truncation hook (cap tool output at 50 KB before it enters context):"
        );
        let _ = writeln!(
            out,
            r#"  {{
    "hooks": {{
      "PostToolUse": [
        {{
          "matcher": ".*",
          "hooks": [
            {{
              "type": "command",
              "command": "input=$(cat); limit=51200; if [ \"$(printf '%s' \"$input\" | wc -c)\" -gt \"$limit\" ]; then printf '%s' \"$input\" | head -c \"$limit\"; echo \"\\n[tolkin: output truncated at 50 KB]\"; else printf '%s' \"$input\"; fi"
            }}
          ]
        }}
      ]
    }}
  }}"#
        );
        let _ = writeln!(out);
    } else {
        let total_events: usize = r.hooks.iter().map(|h| h.events.len()).sum();
        let _ = writeln!(
            out,
            "Hooks: {} file(s) configured ({} event type(s))",
            r.hooks.len(),
            total_events
        );
        for h in &r.hooks {
            if h.events.is_empty() {
                let _ = writeln!(
                    out,
                    "  {} ({}): hooks key present, no event types configured",
                    scan::display_path(&h.path, home),
                    h.scope
                );
            } else {
                let _ = writeln!(
                    out,
                    "  {} ({}): {}",
                    scan::display_path(&h.path, home),
                    h.scope,
                    h.events.join(", ")
                );
            }
        }
    }
    let _ = writeln!(out);

    for w in &r.warnings {
        let _ = writeln!(out, "warn: {w}");
    }
    let _ = writeln!(out);

    let save_tokens: u64 = r.mcp.iter().map(|m| m.analysis.totals.savings_tokens).sum();
    let save_usd: f64 = r.mcp.iter().map(|m| m.analysis.totals.savings_usd).sum();
    if save_tokens > 0 {
        let _ = writeln!(
            out,
            "Total reclaimable (MCP swaps): ~{} tokens/session, ~{}/session at {} input rates",
            commas(save_tokens),
            usd(save_usd),
            provider.slug()
        );
    } else {
        let _ = writeln!(out, "Total reclaimable (MCP swaps): none found");
    }
    if deep {
        let _ = writeln!(out, "Run tolkin mcp <file> for per-server detail.");
    } else {
        let _ = writeln!(
            out,
            "Run tolkin mcp <file> for per-server detail, tolkin scan --deep for instruction-file audits."
        );
    }
    out
}

fn to_json(r: &ScanReport, provider: TokProvider) -> Value {
    let mcp: Vec<Value> = r
        .mcp
        .iter()
        .map(|m| {
            json!({
                "client": m.client,
                "path": m.path,
                "analysis": serde_json::to_value(&m.analysis).unwrap_or(Value::Null),
                "swaps": m.swaps.iter().map(|s| json!({
                    "server": s.server,
                    "cli": s.cli,
                    "binary": s.binary,
                    "installed": s.installed,
                    "install_hint": s.install_hint,
                    "savings_tokens": s.savings_tokens,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let instruction_files: Vec<Value> = r
        .instruction_files
        .iter()
        .map(|f| {
            let mut v = json!({ "path": f.path, "tokens": f.tokens });
            if let Some(a) = &f.audit {
                v["audit"] = serde_json::to_value(a).unwrap_or(Value::Null);
            }
            v
        })
        .collect();

    let shell: Vec<Value> = r
        .shell
        .iter()
        .map(|s| json!({ "path": s.path, "secret_count": s.secret_count, "kinds": s.kinds }))
        .collect();

    let workflows_llm: Vec<Value> = r
        .workflows_llm
        .iter()
        .map(|w| {
            json!({
                "path": w.path,
                "prompt_tokens": w.prompt_tokens,
                "prompt_fields": w.prompt_fields.iter().map(|f| json!({
                    "field": f.field,
                    "tokens": f.tokens,
                })).collect::<Vec<_>>(),
                "note": "Workflow prompts load per-CI-run, not per-agent-session. These tokens are not included in always-loaded totals.",
            })
        })
        .collect();

    let hooks: Vec<Value> = r
        .hooks
        .iter()
        .map(|h| {
            json!({
                "path": h.path,
                "scope": h.scope,
                "events": h.events,
            })
        })
        .collect();

    json!({
        "mcp": mcp,
        "instruction_files": instruction_files,
        "shell": shell,
        "workflows_llm": workflows_llm,
        "hooks": hooks,
        "environment": r.environment,
        "warnings": r.warnings,
        "totals": {
            "mcp_configs": r.mcp.len(),
            "mcp_cold_tokens": r.mcp.iter().map(|m| m.analysis.totals.cold).sum::<u64>(),
            "reclaimable_tokens": r.mcp.iter().map(|m| m.analysis.totals.savings_tokens).sum::<u64>(),
            "reclaimable_usd": r.mcp.iter().map(|m| m.analysis.totals.savings_usd).sum::<f64>(),
            "instruction_files": r.instruction_files.len(),
            "instruction_tokens": r.instruction_files.iter().map(|f| f.tokens).sum::<u64>(),
            "shell_secret_count": r.shell.iter().map(|s| s.secret_count).sum::<usize>(),
            "workflows_llm_files": r.workflows_llm.len(),
            "workflows_llm_prompt_tokens": r.workflows_llm.iter().map(|w| w.prompt_tokens).sum::<u64>(),
            "hooks_files": r.hooks.len(),
            "provider": provider.slug(),
        },
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{HooksFinding, InstructionFile, McpFinding, ShellFinding, SwapInfo};
    use std::path::PathBuf;
    use tolkin_core::mcp;

    fn empty_report() -> ScanReport {
        ScanReport {
            mcp: vec![],
            instruction_files: vec![],
            shell: vec![],
            workflows_llm: vec![],
            hooks: vec![],
            environment: vec![],
            warnings: vec![],
        }
    }

    fn populated_report() -> ScanReport {
        let config = r#"{ "mcpServers": {
            "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] }
        } }"#;
        let analysis = mcp::analyze(config, CoreProvider::Anthropic).unwrap();
        let swaps = vec![SwapInfo {
            server: "github".to_string(),
            cli: "gh".to_string(),
            binary: "gh".to_string(),
            installed: true,
            install_hint: "brew install gh".to_string(),
            savings_tokens: 40_000,
        }];
        ScanReport {
            mcp: vec![McpFinding {
                client: "Cursor",
                path: PathBuf::from("/home/u/.cursor/mcp.json"),
                analysis,
                swaps,
            }],
            instruction_files: vec![InstructionFile {
                path: PathBuf::from("/home/u/.claude/CLAUDE.md"),
                tokens: 1_204,
                audit: None,
            }],
            shell: vec![ShellFinding {
                path: PathBuf::from("/home/u/.zshrc"),
                secret_count: 2,
                kinds: vec!["openai-key".to_string(), "high-entropy".to_string()],
            }],
            workflows_llm: vec![],
            hooks: vec![],
            environment: vec!["node".to_string(), "gh".to_string()],
            warnings: vec![],
        }
    }

    fn report_with_hooks() -> ScanReport {
        let mut r = empty_report();
        r.hooks = vec![HooksFinding {
            path: PathBuf::from("/home/u/.claude/settings.json"),
            scope: "user",
            events: vec!["PostToolUse".to_string(), "PreToolUse".to_string()],
        }];
        r
    }

    #[test]
    fn empty_scan_renders_calm_report() {
        let out = render(
            &empty_report(),
            Path::new("/home/u"),
            TokProvider::Anthropic,
            false,
        );
        assert!(out.contains("Nothing leaves this machine"));
        assert!(out.contains("MCP configs (0 found)"));
        assert!(out.contains("Instruction files (0 found"));
        assert!(out.contains("Total reclaimable (MCP swaps): none found"));
        assert!(!out.contains("warn:"));
    }

    #[test]
    fn no_hooks_renders_recommendation_with_templates() {
        let out = render(
            &empty_report(),
            Path::new("/home/u"),
            TokProvider::Anthropic,
            false,
        );
        assert!(out.contains("Hooks: none configured"));
        assert!(out.contains("PreToolUse"));
        assert!(out.contains("PostToolUse"));
        assert!(out.contains("code.claude.com/docs/en/hooks"));
    }

    #[test]
    fn hooks_present_renders_neutral_fact() {
        let out = render(
            &report_with_hooks(),
            Path::new("/home/u"),
            TokProvider::Anthropic,
            false,
        );
        assert!(out.contains("Hooks: 1 file(s) configured (2 event type(s))"));
        assert!(out.contains("PostToolUse, PreToolUse"));
        assert!(!out.contains("none configured"));
    }

    #[test]
    fn populated_report_renders_swaps_and_secret_kinds_only() {
        let out = render(
            &populated_report(),
            Path::new("/home/u"),
            TokProvider::Anthropic,
            false,
        );
        assert!(out.contains("Cursor  ~/.cursor/mcp.json"));
        assert!(out.contains("swap github -> gh (gh is installed, zero-setup swap)"));
        assert!(out.contains(
            "1 server can be slimmed for ~20,000 tokens; run tolkin mcp ~/.cursor/mcp.json for snippets"
        ));
        assert!(out.contains("~/.zshrc: 2 likely secrets exported (openai-key, high-entropy)"));
        assert!(out.contains("Environment: node, gh detected"));
        assert!(out.contains("Total reclaimable (MCP swaps): ~40,000 tokens/session"));
    }

    #[test]
    fn json_shape_serializes_with_expected_keys() {
        let v = to_json(&populated_report(), TokProvider::Anthropic);
        for key in [
            "mcp",
            "instruction_files",
            "shell",
            "hooks",
            "environment",
            "warnings",
            "totals",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["mcp"][0]["client"], "Cursor");
        assert_eq!(v["mcp"][0]["swaps"][0]["installed"], true);
        assert!(v["mcp"][0]["analysis"]["totals"]["cold"].is_u64());
        assert_eq!(v["instruction_files"][0]["tokens"], 1_204);
        assert!(v["instruction_files"][0].get("audit").is_none());
        assert_eq!(v["shell"][0]["secret_count"], 2);
        assert_eq!(v["totals"]["provider"], "anthropic");
        assert_eq!(v["totals"]["reclaimable_tokens"], 40_000);
        assert!(v["totals"]["hooks_files"].is_u64());
        // round-trips through a serializer
        let s = serde_json::to_string(&v).unwrap();
        let _: Value = serde_json::from_str(&s).unwrap();
    }

    #[test]
    fn hooks_json_carries_path_scope_events() {
        let v = to_json(&report_with_hooks(), TokProvider::Anthropic);
        let hooks = v["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["scope"], "user");
        let events = hooks[0]["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(v["totals"]["hooks_files"], 1);
    }
}
