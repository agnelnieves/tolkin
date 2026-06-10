use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Args;
use serde_json::json;
use tolkin_core::mcp::{self, McpAnalysis, Recommendation};
use tolkin_core::Provider as CoreProvider;

use crate::input;
use crate::ledger;
use crate::tokenize::Provider as TokProvider;

#[derive(Args, Debug)]
pub struct McpArgs {
    /// MCP config file (JSON or JSONC), or "-" for stdin. Defaults to stdin.
    pub file: Option<PathBuf>,

    /// Provider for the cold/warm cache math (anthropic carries the cache surcharge).
    #[arg(long, default_value = "anthropic")]
    pub provider: TokProvider,

    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: McpArgs) -> Result<()> {
    let target = args
        .file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "stdin".to_string());
    let text = input::read(args.file.as_deref())?;
    let provider = core_provider(args.provider);
    let analysis = mcp::analyze(&text, provider).map_err(|e| anyhow!(e))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&analysis)?);
    } else {
        print_report(&analysis);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    ledger::append(
        "mcp",
        &cwd,
        json!({
            "servers": analysis.totals.servers as u64,
            "cold_tokens": analysis.totals.cold,
            "swap_savings_tokens": analysis.totals.savings_tokens,
            "slim_savings_tokens": analysis.totals.slim_savings_tokens,
            "target": target,
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

fn rec_label(r: Recommendation) -> &'static str {
    match r {
        Recommendation::Replace => "replace",
        Recommendation::ReplaceForAdHoc => "replace*",
        Recommendation::Keep => "keep",
        Recommendation::Unknown => "unknown",
    }
}

fn print_report(a: &McpAnalysis) {
    println!("MCP config: {}", a.client);
    println!("Provider:   {} (cache math)\n", a.provider.display());

    println!(
        "{:<16} {:<9} {:>6} {:>9} {:>9}  CLI swap",
        "Server", "Action", "Tools", "Cold", "Saves"
    );
    for s in &a.servers {
        let tools = s
            .tools
            .map_or_else(|| "-".to_string(), |t| commas(u64::from(t)));
        let cold = s
            .scenarios
            .as_ref()
            .map_or_else(|| "-".to_string(), |sc| commas(sc.cold));
        let saves = if s.savings_tokens > 0 {
            commas(s.savings_tokens)
        } else {
            "-".to_string()
        };
        let cli = s.cli_alternative.clone().unwrap_or_else(|| "-".to_string());
        println!(
            "{:<16} {:<9} {:>6} {:>9} {:>9}  {}",
            truncate(&s.name, 16),
            rec_label(s.recommendation),
            tools,
            cold,
            saves,
            cli,
        );
        if let Some(slim) = &s.slim {
            if slim.already_slimmed {
                println!(
                    "  already slimmed ({} set); cold estimate adjusted",
                    slim.option.mechanism
                );
            } else if slim.est_tokens_saved > 0 {
                println!(
                    "  slim: save ~{} tokens with {} (if you keep it, slim it)",
                    commas(slim.est_tokens_saved),
                    slim.option.mechanism
                );
                println!("    {}", slim.option.snippet);
            } else {
                println!("  slim: {}", slim.option.snippet);
            }
        } else if s.note.contains("No native tool filtering") {
            // Mirrors the sentence the core appends to verified
            // no-filtering catalog entries.
            println!("  no native filtering; slim client-side with MCP tool search (see notes)");
        }
    }

    let t = &a.totals;
    println!();
    println!(
        "Totals: cold {} tok ({}% of a 200K window), warm {}, Tool Search {}",
        commas(t.cold),
        t.pct_of_window,
        commas(t.warm),
        commas(t.tool_search)
    );
    let swaps: Vec<&_> = a.servers.iter().filter(|s| s.savings_tokens > 0).collect();
    if t.savings_tokens > 0 {
        println!(
            "Potential savings: {} input tokens ({} per cold session) by swapping {} server(s) to their CLIs.",
            commas(t.savings_tokens),
            usd(t.savings_usd),
            swaps.len()
        );
    } else {
        println!("No CLI-swap savings found in this config.");
    }
    if t.slim_savings_tokens > 0 {
        println!(
            "Slim savings (keep the servers, register fewer tools): ~{} input tokens ({} per cold session). Per server, swap and slim are alternatives, not additive.",
            commas(t.slim_savings_tokens),
            usd(t.slim_savings_usd)
        );
    }

    if !swaps.is_empty() {
        println!("\nRecommendations:");
        for s in &swaps {
            println!(
                "  {} to `{}`: {}",
                s.name,
                s.cli_alternative.clone().unwrap_or_default(),
                s.note
            );
        }
    }

    println!("\nNotes:");
    for n in &a.notes {
        println!("  - {n}");
    }
    if a.servers
        .iter()
        .any(|s| s.recommendation == Recommendation::ReplaceForAdHoc)
    {
        println!("\n* replace for ad hoc use; keep the MCP for the flows noted above.");
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
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
        format!("${v:.4}")
    } else {
        format!("${v:.2}")
    }
}
