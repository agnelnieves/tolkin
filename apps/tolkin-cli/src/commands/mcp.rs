use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use clap::Args;
use serde_json::json;
use tolkin_core::mcp::{self, McpAnalysis, Recommendation};
use tolkin_core::mcp_tools::{self, ToolInventory, ToolTokenCount};
use tolkin_core::Provider as CoreProvider;

use crate::input;
use crate::ledger;
use crate::tokenize::{self, Provider as TokProvider};

#[derive(Args, Debug)]
pub struct McpArgs {
    /// MCP config file (JSON or JSONC) or a tools/list manifest, or "-" for
    /// stdin. Defaults to stdin. A tools/list shape ({"tools": [...]}, a bare
    /// tool array, or a JSON-RPC envelope) is detected automatically and
    /// analyzed as a single server with exact tokenized counts.
    pub file: Option<PathBuf>,

    /// Provider for the cold/warm cache math (anthropic carries the cache
    /// surcharge). When set, it also selects the manifest tokenizer; manifest
    /// counts default to o200k_base (exact) otherwise.
    #[arg(long)]
    pub provider: Option<TokProvider>,

    /// tools/list JSON for one server (path or "-" for stdin). Without a
    /// config file this analyzes the manifest as a single server; with a
    /// config it attaches exact counts to the targeted server (see --server).
    #[arg(long, value_name = "PATH")]
    pub tools_list: Option<PathBuf>,

    /// Server name the tools/list belongs to. Required with --tools-list when
    /// the config has more than one server; names the server in single
    /// manifest mode (default: the manifest filename).
    #[arg(long, value_name = "NAME")]
    pub server: Option<String>,

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
    let cache_provider = core_provider(args.provider.unwrap_or(TokProvider::Anthropic));
    // Manifest tokenization defaults to o200k_base (exact). An explicit
    // --provider also picks that provider's tokenizer; the basis label names
    // whichever ran, and the Anthropic proxy stays labeled an estimate.
    let tok_provider = args.provider.unwrap_or(TokProvider::OpenAi);

    let analysis = match &args.tools_list {
        Some(manifest_path) => {
            let manifest_is_stdin = manifest_path.as_os_str() == "-";
            let file_is_stdin = args.file.as_ref().is_some_and(|p| p.as_os_str() == "-");
            if manifest_is_stdin && file_is_stdin {
                bail!("--tools-list - and a stdin config cannot both read stdin; give one of them a file path");
            }
            let manifest_text = input::read(Some(manifest_path.as_path()))?;
            let inventory = tokenize_inventory(&manifest_text, tok_provider)?;

            match &args.file {
                None => {
                    // Single-server analysis straight from the manifest: the
                    // common case, no config file needed.
                    let name = args
                        .server
                        .clone()
                        .or_else(|| manifest_server_name(Some(manifest_path)))
                        .unwrap_or_else(|| "server".to_string());
                    mcp::analysis_from_inventory(&name, cache_provider, inventory)
                }
                Some(config_path) => {
                    let config_text = input::read(Some(config_path.as_path()))?;
                    let names = mcp::server_names(&config_text).map_err(|e| anyhow!(e))?;
                    let target_server = match &args.server {
                        Some(name) => name.clone(),
                        None if names.len() == 1 => names[0].clone(),
                        None => bail!(
                            "the config has {} servers ({}); pass --server <name> to say which one the tools/list belongs to",
                            names.len(),
                            names.join(", ")
                        ),
                    };
                    let mut inventories = BTreeMap::new();
                    inventories.insert(target_server, inventory);
                    mcp::analyze_with_inventories(&config_text, cache_provider, &inventories)
                        .map_err(|e| anyhow!(e))?
                }
            }
        }
        None => {
            let text = input::read(args.file.as_deref())?;
            if mcp_tools::is_tools_list(&text) {
                // The positional file IS a tools/list manifest: single-server
                // exact analysis without any flag.
                let name = args
                    .server
                    .clone()
                    .or_else(|| manifest_server_name(args.file.as_deref()))
                    .unwrap_or_else(|| "server".to_string());
                let inventory = tokenize_inventory(&text, tok_provider)?;
                mcp::analysis_from_inventory(&name, cache_provider, inventory)
            } else {
                mcp::analyze(&text, cache_provider).map_err(|e| anyhow!(e))?
            }
        }
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&analysis)?);
    } else {
        print_report(&analysis);
    }

    let measured = analysis.totals.measured as u64;
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    ledger::append(
        "mcp",
        &cwd,
        json!({
            "servers": analysis.totals.servers as u64,
            "cold_tokens": analysis.totals.cold,
            "swap_savings_tokens": analysis.totals.savings_tokens,
            "slim_savings_tokens": analysis.totals.slim_savings_tokens,
            "measured_servers": measured,
            "target": target,
        }),
    );
    Ok(())
}

/// Tokenize every tool in a tools/list payload with the platform tokenizer:
/// the canonical compact serialization (counted as `tokens`) and the bare
/// description (counted as `description_tokens`). The core does the parsing
/// and the analysis; only the counting happens here.
fn tokenize_inventory(manifest_text: &str, provider: TokProvider) -> Result<ToolInventory> {
    let specs = mcp_tools::parse_tools_list(manifest_text).map_err(|e| anyhow!(e))?;
    let mut counts = Vec::with_capacity(specs.len());
    for spec in &specs {
        counts.push(ToolTokenCount {
            name: spec.name.clone(),
            description: spec.description.clone(),
            tokens: tokenize::count(provider, &spec.serialized)? as u64,
            description_tokens: tokenize::count(provider, &spec.description)? as u64,
        });
    }
    mcp_tools::analyze_tool_inventory(&counts, tokenizer_label(provider)).map_err(|e| anyhow!(e))
}

/// Tokenizer label embedded in the basis string. The Anthropic proxy is
/// labeled an estimate everywhere it appears; o200k_base and Gemma are exact.
fn tokenizer_label(p: TokProvider) -> &'static str {
    match p {
        TokProvider::OpenAi => "o200k_base",
        TokProvider::Anthropic => "cl100k_base proxy, Claude estimate +/-10%",
        TokProvider::Gemini => "Gemma SPM",
    }
}

/// Server name for a manifest given its path: the file stem, with a trailing
/// ".tools" dropped (so server-github.tools.json names server-github).
fn manifest_server_name(path: Option<&Path>) -> Option<String> {
    let stem = path?.file_stem()?.to_str()?;
    let name = stem.strip_suffix(".tools").unwrap_or(stem);
    if name == "-" || name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
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
        if s.tools_detail.is_some() {
            println!(
                "  basis: {} (exact; supersedes the catalog estimate)",
                s.basis
            );
        }
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

    for s in &a.servers {
        if let Some(detail) = &s.tools_detail {
            print_tools_detail(&s.name, detail);
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

/// Per-tool table and description smells for one measured server.
fn print_tools_detail(server: &str, detail: &ToolInventory) {
    println!(
        "\nPer-tool breakdown: {server}, {} tools, {} tokens total [{}]",
        detail.tool_count,
        commas(detail.total_tokens),
        detail.basis
    );
    println!(
        "{:<40} {:>8} {:>7} {:>6}",
        "Tool", "Tokens", "Share", "Desc"
    );
    for row in &detail.tools {
        println!(
            "{:<40} {:>8} {:>6.2}% {:>6}",
            truncate(&row.name, 40),
            commas(row.tokens),
            row.share_pct,
            commas(row.description_tokens),
        );
    }

    if detail.smells.is_empty() {
        println!("\nDescription smells: none found.");
    } else {
        println!("\nDescription smells ({}):", detail.smells.len());
        for smell in &detail.smells {
            println!("  [{}] {}", smell.rule, name_list(&smell.tools, 6));
            println!("    {}", smell.detail);
            println!("    fix: {}", smell.recommendation);
        }
    }
    for n in &detail.notes {
        println!("  - {n}");
    }
}

/// "a, b, c and 4 more" style list capped at `max` names.
fn name_list(names: &[String], max: usize) -> String {
    if names.len() <= max {
        return names.join(", ");
    }
    let shown = names[..max].join(", ");
    format!("{shown} and {} more", names.len() - max)
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
