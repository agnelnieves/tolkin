use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use tolkin_core::mcp::{self, McpAnalysis, Recommendation};
use tolkin_core::mcp_tools::{self, ToolInventory, ToolTokenCount};
use tolkin_core::Provider as CoreProvider;

use crate::input;
use crate::ledger;
use crate::mcp_manifests::{self, Manifest};
use crate::mcp_probe::{self, HttpTarget, ProbeResult, StdioTarget};
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

    /// Probe a configured MCP server live to capture an exact tools/list and
    /// cache it under .tolkin/mcp-manifests/. Repeatable. Requires a config
    /// file. Refused under CI (set committed manifests instead).
    #[arg(long, value_name = "SERVER")]
    pub probe: Vec<String>,

    /// Probe every server in the config. Mutually inclusive with --probe.
    #[arg(long, conflicts_with = "probe")]
    pub probe_all: bool,

    /// Per-server probe deadline in seconds (initialize + tools/list pages).
    #[arg(long, value_name = "SECS", default_value_t = mcp_probe::default_timeout_secs())]
    pub probe_timeout: u64,
}

pub fn run(args: McpArgs, yes: bool) -> Result<()> {
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

    // Project root for the committable manifest cache. cwd matches the rest
    // of the CLI's "this repo" surfaces (project, scan).
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Probe pipeline: runs before any analysis so the cache is up to date
    // when the analyzer goes to resolve inventories. CI refusal happens
    // before any user-facing confirmation.
    let probe_requested = args.probe_all || !args.probe.is_empty();
    if probe_requested {
        if ledger::disabled_by_env() && is_ci_set() {
            bail!(
                "tolkin mcp --probe is refused under CI. CI is set in this environment; commit a manifest under .tolkin/mcp-manifests/ instead so teammates and CI get exact counts from the cache alone."
            );
        }
        let Some(config_path) = args.file.as_deref() else {
            bail!(
                "--probe needs a config file; tolkin mcp --probe reads servers from your own MCP config and would never spawn an unconfigured process"
            );
        };
        if config_path.as_os_str() == "-" {
            bail!(
                "--probe needs a config file (not stdin); the probe runs the exact command lines from your MCP config, which has to exist on disk for the confirmation to mean anything"
            );
        }
        let config_text = std::fs::read_to_string(config_path)
            .with_context(|| format!("read MCP config {}", config_path.display()))?;
        run_probe_pipeline(
            &config_text,
            &args.probe,
            args.probe_all,
            args.probe_timeout,
            &project_root,
            yes,
        )?;
    }

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
                    // The explicit --tools-list flag wins over the cache for
                    // the targeted server (basis resolution rule 1). Cached
                    // manifests for every OTHER server still apply, so a
                    // single targeted manifest does not blow away the cache.
                    let mut inventories =
                        load_cached_inventories(&project_root, &names, tok_provider);
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
                // Config path with no --tools-list: cached manifests for
                // any server in the config supersede the catalog estimate.
                let names = mcp::server_names(&text).map_err(|e| anyhow!(e))?;
                let inventories = load_cached_inventories(&project_root, &names, tok_provider);
                if inventories.is_empty() {
                    mcp::analyze(&text, cache_provider).map_err(|e| anyhow!(e))?
                } else {
                    mcp::analyze_with_inventories(&text, cache_provider, &inventories)
                        .map_err(|e| anyhow!(e))?
                }
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
        // Unknown servers carry an actionable note pointing at --tools-list;
        // the json output already surfaces it, the plain-text report kept
        // the row blank and dropped the guidance. Print it so the user can
        // act on the unknown row without re-running with --json.
        if s.recommendation == Recommendation::Unknown {
            println!("  {}", s.note);
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

// ---------------------------------------------------------------------------
// Probe pipeline
// ---------------------------------------------------------------------------

/// True when CI is set to anything truthy. Tracks the same convention as
/// `ledger::disabled_by_env` so the probe refusal aligns with the rest of
/// the CLI's CI-aware paths.
fn is_ci_set() -> bool {
    match std::env::var("CI") {
        Ok(v) => {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        }
        Err(_) => false,
    }
}

/// One probe target resolved from the raw MCP config. `Stdio` and `Http`
/// carry the real command / env / headers values; this struct never leaves
/// the CLI process (the cached manifest stores zero of these fields).
#[derive(Debug)]
enum ProbeTarget {
    Stdio(StdioTarget),
    Http(HttpTarget),
}

/// Resolve probe targets from raw config JSON. Returns a map of server name
/// to target for every server the user asked to probe.
fn resolve_probe_targets(
    config_text: &str,
    requested: &[String],
    probe_all: bool,
) -> Result<BTreeMap<String, ProbeTarget>> {
    let cleaned = strip_jsonc_simple(config_text);
    let value: Value = serde_json::from_str(&cleaned)
        .map_err(|e| anyhow!("could not parse config as JSON/JSONC: {e}"))?;
    let servers_obj = locate_servers_object(&value)
        .ok_or_else(|| anyhow!("no MCP servers section found in config"))?;

    let mut out: BTreeMap<String, ProbeTarget> = BTreeMap::new();
    let names: Vec<String> = servers_obj.keys().cloned().collect();
    let wanted: BTreeSet<String> = if probe_all {
        names.iter().cloned().collect()
    } else {
        requested.iter().cloned().collect()
    };

    for name in &wanted {
        let Some(entry) = servers_obj.get(name) else {
            bail!(
                "--probe {name}: not in this config. Known servers: {}",
                names.join(", ")
            );
        };
        let target = extract_target(name, entry)?;
        out.insert(name.clone(), target);
    }
    Ok(out)
}

/// Strip JSONC comments with a small inline pass so we do not need to
/// re-export the core's strip_jsonc. Kept conservative: this is just for
/// probe target extraction.
fn strip_jsonc_simple(text: &str) -> String {
    // Round-trip through the analyzer's server-name path: that already
    // strips JSONC, so we can just re-parse via serde_json. But we need the
    // raw values here, so we have to do it ourselves. Cheap approximation:
    // line-comment removal only; block comments and trailing commas in MCP
    // configs are rare enough that this covers the common cases. If parsing
    // fails downstream the error is clear.
    let mut out = String::with_capacity(text.len());
    let mut in_str = false;
    let mut esc = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for n in chars.by_ref() {
                if n == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Locate the server map under any known client key. Mirrors the core's
/// `locate_servers` but returns the map directly so we can iterate keys.
fn locate_servers_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    if let Some(v) = value.get("mcpServers").and_then(Value::as_object) {
        return Some(v);
    }
    if let Some(v) = value
        .get("mcp")
        .and_then(|m| m.get("servers"))
        .and_then(Value::as_object)
    {
        return Some(v);
    }
    if let Some(v) = value.get("servers").and_then(Value::as_object) {
        return Some(v);
    }
    if let Some(v) = value.get("context_servers").and_then(Value::as_object) {
        return Some(v);
    }
    None
}

/// Build a ProbeTarget from one server entry. Reads url + headers when
/// present (HTTP), command + args + env otherwise (stdio). Zed's command
/// object form `{ "path": ..., "args": ... }` is honored.
fn extract_target(name: &str, entry: &Value) -> Result<ProbeTarget> {
    let url = entry.get("url").and_then(Value::as_str).map(String::from);
    if let Some(url) = url {
        let headers = entry
            .get("headers")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        return Ok(ProbeTarget::Http(HttpTarget { url, headers }));
    }

    // stdio.
    let (command, mut args) = if let Some(cmd_obj) = entry.get("command").and_then(Value::as_object)
    {
        let path = cmd_obj
            .get("path")
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| anyhow!("server {name}: command object has no path"))?;
        let args = cmd_obj
            .get("args")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        (path, args)
    } else if let Some(s) = entry.get("command").and_then(Value::as_str) {
        (s.to_string(), Vec::new())
    } else {
        bail!("server {name}: no url or command, nothing to probe");
    };
    if args.is_empty() {
        if let Some(a) = entry.get("args").and_then(Value::as_array) {
            args = a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
    }
    let env = entry
        .get("env")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    Ok(ProbeTarget::Stdio(StdioTarget { command, args, env }))
}

/// Run the probe pipeline: confirm, probe, redact, save. Per-server output
/// goes to stdout so the user sees exactly which servers were touched.
fn run_probe_pipeline(
    config_text: &str,
    requested: &[String],
    probe_all: bool,
    timeout_secs: u64,
    project_root: &Path,
    yes: bool,
) -> Result<()> {
    let targets = resolve_probe_targets(config_text, requested, probe_all)?;
    if targets.is_empty() {
        bail!("--probe: no servers selected");
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "Probe plan: {} server(s)", targets.len());

    for (name, target) in &targets {
        let (transport_label, line) = match target {
            ProbeTarget::Stdio(t) => ("stdio", mcp_probe::stdio_confirmation_line(t)),
            ProbeTarget::Http(t) => ("http", mcp_probe::http_confirmation_line(t)),
        };
        let _ = writeln!(out, "\n  {name} ({transport_label}):");
        let _ = writeln!(out, "    {line}");

        if !yes {
            let _ = write!(out, "Run this probe? [y/N] ");
            let _ = out.flush();
            let stdin = io::stdin();
            let mut answer = String::new();
            if stdin.lock().read_line(&mut answer).is_err() {
                let _ = writeln!(out, "  {name}: skipped (no input)");
                continue;
            }
            match answer.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => {}
                _ => {
                    let _ = writeln!(out, "  {name}: skipped");
                    continue;
                }
            }
        }

        let (transport_str, probe_outcome) = match target {
            ProbeTarget::Stdio(t) => ("stdio", mcp_probe::probe_stdio(t, timeout_secs)),
            ProbeTarget::Http(t) => ("http", mcp_probe::probe_http(t, timeout_secs)),
        };
        let probe = match probe_outcome {
            Ok(p) => p,
            Err(e) => {
                let _ = writeln!(out, "  {name}: probe failed: {e}");
                continue;
            }
        };
        save_and_report(&mut out, name, transport_str, probe, project_root)?;
    }
    let _ = writeln!(out);
    Ok(())
}

fn save_and_report<W: Write>(
    out: &mut W,
    name: &str,
    transport: &str,
    probe: ProbeResult,
    project_root: &Path,
) -> Result<()> {
    let manifest = mcp_manifests::build_manifest(
        name,
        transport,
        "probe",
        probe.protocol_version.clone(),
        probe.tools.clone(),
    )?;
    let path = mcp_manifests::save_manifest(project_root, &manifest)?;
    let _ = writeln!(
        out,
        "  {name}: captured {} tool(s); saved to {}",
        manifest.tools.len(),
        path.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Cache resolution (every analysis run)
// ---------------------------------------------------------------------------

/// Load cached manifests for the named servers, tokenize each, and rewrite
/// the basis to name the cache + capture date. This is the resolution rule
/// "explicit flag wins, then cache, then catalog, then none" for every
/// server whose cache hits (the caller still applies the flag-wins step).
fn load_cached_inventories(
    project_root: &Path,
    server_names: &[String],
    tok_provider: TokProvider,
) -> BTreeMap<String, ToolInventory> {
    let today = mcp_manifests::today_utc();
    let mut out = BTreeMap::new();
    for name in server_names {
        let Some(manifest) = mcp_manifests::load_for(project_root, name) else {
            continue;
        };
        let manifest_text = mcp_manifests::to_tools_list_text(&manifest);
        let Ok(mut inventory) = tokenize_inventory(&manifest_text, tok_provider) else {
            continue;
        };
        inventory.basis = cache_basis(&manifest, &today);
        out.insert(name.clone(), inventory);
    }
    out
}

/// Basis string for a cache hit. Adds the staleness warning when the
/// captured_at date is older than the threshold.
pub(crate) fn cache_basis(manifest: &Manifest, today: &str) -> String {
    let base = format!(
        "measured (probed manifest, captured {})",
        manifest.captured_at
    );
    match mcp_manifests::staleness(manifest, today) {
        Some(warn) => format!("{base}; warning: {warn}"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_probe_targets_picks_named_servers() {
        let cfg = r#"{
            "mcpServers": {
                "tracker": { "command": "node", "args": ["./srv.js"], "env": { "API": "x" } },
                "github":  { "command": "npx", "args": ["@modelcontextprotocol/server-github"] }
            }
        }"#;
        let targets = resolve_probe_targets(cfg, &["tracker".to_string()], false).unwrap();
        assert_eq!(targets.len(), 1);
        match targets.get("tracker").unwrap() {
            ProbeTarget::Stdio(t) => {
                assert_eq!(t.command, "node");
                assert_eq!(t.args, vec!["./srv.js".to_string()]);
                assert_eq!(t.env.get("API").map(|s| s.as_str()), Some("x"));
            }
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn resolve_probe_targets_probe_all_returns_every_entry() {
        let cfg = r#"{ "mcpServers": {
            "a": { "url": "https://a.test/mcp", "headers": { "Authorization": "Bearer xyz" } },
            "b": { "command": "x" }
        } }"#;
        let targets = resolve_probe_targets(cfg, &[], true).unwrap();
        assert_eq!(targets.len(), 2);
        match targets.get("a").unwrap() {
            ProbeTarget::Http(t) => {
                assert_eq!(t.url, "https://a.test/mcp");
                assert!(t.headers.contains_key("Authorization"));
            }
            _ => panic!("expected http for a"),
        }
    }

    #[test]
    fn resolve_probe_targets_unknown_server_is_a_hard_error() {
        let cfg = r#"{ "mcpServers": { "tracker": { "command": "x" } } }"#;
        let err = resolve_probe_targets(cfg, &["nope".to_string()], false).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("nope"));
        assert!(msg.contains("tracker"));
    }

    #[test]
    fn cache_basis_includes_capture_date_and_optional_warning() {
        let mut m = Manifest {
            v: 1,
            server: "x".to_string(),
            captured_at: "2026-06-11".to_string(),
            transport: "stdio".to_string(),
            source: "probe".to_string(),
            protocol_version: None,
            tools: Vec::new(),
        };
        let fresh = cache_basis(&m, "2026-06-12");
        assert!(fresh.contains("measured (probed manifest, captured 2026-06-11)"));
        assert!(!fresh.contains("warning"), "{fresh}");

        m.captured_at = "2025-01-01".to_string();
        let stale = cache_basis(&m, "2026-06-11");
        assert!(stale.contains("warning"));
        assert!(stale.contains("re-probe"));
    }
}
