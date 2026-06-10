//! Local agent-config discovery for `tolkin scan`. Strictly read-only: every
//! collector reads files and reports on them; nothing is written, moved, or
//! transmitted, and no external binary is spawned (PATH probing is a plain
//! file-permission check).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tolkin_core::audit::{self, AuditOptions, AuditReport};
use tolkin_core::mcp::{self, McpAnalysis};
use tolkin_core::redact::{self, RedactOptions};
use tolkin_core::Provider as CoreProvider;

use crate::tokenize::{self, Provider as TokProvider};

// ---------------------------------------------------------------------------
// OS abstraction
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanOs {
    MacOs,
    Linux,
    Windows,
    Other,
}

impl ScanOs {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "macos" => ScanOs::MacOs,
            "linux" => ScanOs::Linux,
            "windows" => ScanOs::Windows,
            _ => ScanOs::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Resolved roots (injected for tests, detected at runtime)
// ---------------------------------------------------------------------------

pub struct ScanRoots {
    pub home: PathBuf,
    pub cwd: PathBuf,
    /// dirs::config_dir(): ~/Library/Application Support on macOS,
    /// ~/.config on Linux, %APPDATA% on Windows.
    pub config: Option<PathBuf>,
    /// dirs::data_dir(): ~/Library/Application Support on macOS,
    /// ~/.local/share on Linux, %APPDATA% on Windows.
    /// Reserved for the ledger workstream (W2); unused in the scan collector.
    #[allow(dead_code)]
    pub data: Option<PathBuf>,
    pub os: ScanOs,
}

impl ScanRoots {
    /// Detect roots from the running environment. `cwd` is passed in (callers
    /// already have it from `std::env::current_dir()`). Home is required;
    /// config and data are optional (absent on unusual headless setups).
    pub fn detect(cwd: PathBuf) -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(ScanRoots {
            home,
            cwd,
            config: dirs::config_dir(),
            data: dirs::data_dir(),
            os: ScanOs::current(),
        })
    }
}

// ---------------------------------------------------------------------------
// Catalog types
// ---------------------------------------------------------------------------

/// Which root a catalog entry is relative to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Home,
    Cwd,
    /// dirs::config_dir() (platform-correct). Skipped when config root is None.
    Config,
}

/// One concrete path within an McpSource, optionally restricted to a subset
/// of operating systems.
#[derive(Clone, Copy, Debug)]
pub struct Location {
    pub base: Base,
    pub rel: &'static str,
    /// None means "all OSes".
    pub only: Option<&'static [ScanOs]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Pass the file text straight to `mcp::analyze`; it detects the
    /// mcpServers, servers, mcp.servers, and context_servers shapes itself
    /// (JSONC included), so whole settings files are safe to hand over.
    Auto,
    /// `~/.claude.json` is a large state file. Only the top-level `mcpServers`
    /// and each project entry's `mcpServers` subtrees are extracted and
    /// re-serialized as a small config.
    ClaudeJson,
    /// `~/.codex/config.toml` keeps servers in `[mcp_servers.<name>]` TOML
    /// tables; they are converted to the JSON `mcpServers` shape first.
    CodexToml,
}

pub struct McpSource {
    pub client: &'static str,
    pub locations: &'static [Location],
    pub shape: Shape,
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

// Shorthand OS-filter slices for the catalog. Some are unused in v0.3 but are
// part of the catalog API for upcoming entries (W1 portability workstream).
#[allow(dead_code)]
const MACOS: &[ScanOs] = &[ScanOs::MacOs];
#[allow(dead_code)]
const LINUX: &[ScanOs] = &[ScanOs::Linux];
const WINDOWS: &[ScanOs] = &[ScanOs::Windows];
const MAC_LINUX: &[ScanOs] = &[ScanOs::MacOs, ScanOs::Linux];

pub static MCP_CATALOG: &[McpSource] = &[
    // Claude Desktop: config_dir divergence handles per-OS difference.
    McpSource {
        client: "Claude Desktop",
        locations: &[Location {
            base: Base::Config,
            rel: "Claude/claude_desktop_config.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Claude Code",
        locations: &[Location {
            base: Base::Home,
            rel: ".claude.json",
            only: None,
        }],
        shape: Shape::ClaudeJson,
    },
    McpSource {
        client: "Claude Code",
        locations: &[Location {
            base: Base::Home,
            rel: ".claude/settings.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Claude Code (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".mcp.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Claude Code (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".claude/settings.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Cursor",
        locations: &[Location {
            base: Base::Home,
            rel: ".cursor/mcp.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Cursor (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".cursor/mcp.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Codex",
        locations: &[Location {
            base: Base::Home,
            rel: ".codex/config.toml",
            only: None,
        }],
        shape: Shape::CodexToml,
    },
    // VS Code: config_dir covers macOS (~/Library/Application Support),
    // Linux (~/.config), and Windows (%APPDATA%) with a single rel path.
    McpSource {
        client: "VS Code",
        locations: &[Location {
            base: Base::Config,
            rel: "Code/User/settings.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "VS Code (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".vscode/mcp.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    // Zed: uses ~/.config/zed on macOS and Linux (ignores macOS app support);
    // uses config_dir/Zed on Windows.
    McpSource {
        client: "Zed",
        locations: &[
            Location {
                base: Base::Home,
                rel: ".config/zed/settings.json",
                only: Some(MAC_LINUX),
            },
            Location {
                base: Base::Config,
                rel: "Zed/settings.json",
                only: Some(WINDOWS),
            },
        ],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Zed (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".zed/settings.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Continue",
        locations: &[Location {
            base: Base::Home,
            rel: ".continue/config.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Continue (project)",
        locations: &[Location {
            base: Base::Cwd,
            rel: ".continue/config.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Windsurf",
        locations: &[Location {
            base: Base::Home,
            rel: ".codeium/windsurf/mcp_config.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
    McpSource {
        client: "Gemini CLI",
        locations: &[Location {
            base: Base::Home,
            rel: ".gemini/settings.json",
            only: None,
        }],
        shape: Shape::Auto,
    },
];

const INSTRUCTION_HOME: &[&str] = &[".claude/CLAUDE.md", ".codex/AGENTS.md"];
const INSTRUCTION_CWD: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".cursorrules",
    ".github/copilot-instructions.md",
    "GEMINI.md",
];

const SHELL_CONFIGS: &[&str] = &[".zshrc", ".zprofile", ".zshenv", ".bashrc", ".bash_profile"];

/// Binaries relevant to agent workflows. Feeds the environment summary and
/// gives the MCP swap annotations context.
const ENV_BINARIES: &[&str] = &[
    "node", "bun", "npm", "brew", "gh", "git", "psql", "aws", "kubectl", "docker", "cargo",
];

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SwapInfo {
    pub server: String,
    pub cli: String,
    pub binary: String,
    pub installed: bool,
    pub install_hint: String,
    pub savings_tokens: u64,
}

#[derive(Debug)]
pub struct McpFinding {
    pub client: &'static str,
    pub path: PathBuf,
    pub analysis: McpAnalysis,
    pub swaps: Vec<SwapInfo>,
}

#[derive(Debug)]
pub struct InstructionFile {
    pub path: PathBuf,
    pub tokens: u64,
    pub audit: Option<AuditReport>,
}

/// Counts and kind names only. Values, byte spans, and redacted text are
/// dropped inside the collector and never reach this struct.
#[derive(Debug)]
pub struct ShellFinding {
    pub path: PathBuf,
    pub secret_count: usize,
    pub kinds: Vec<String>,
}

#[derive(Debug)]
pub struct ScanReport {
    pub mcp: Vec<McpFinding>,
    pub instruction_files: Vec<InstructionFile>,
    pub shell: Vec<ShellFinding>,
    pub environment: Vec<String>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn scan(roots: &ScanRoots, path_var: &str, provider: CoreProvider, deep: bool) -> ScanReport {
    let mut warnings = Vec::new();
    let mcp = collect_mcp(roots, path_var, provider, &mut warnings);
    let instruction_files = collect_instructions(roots, deep, &mut warnings);
    let shell = collect_shell(&roots.home, &mut warnings);
    let environment = collect_environment(&roots.cwd, path_var);
    ScanReport {
        mcp,
        instruction_files,
        shell,
        environment,
        warnings,
    }
}

/// Presence-only: returns (client, resolved path) for every catalog file that
/// exists on disk, deduped by resolved path.
/// Called by the onboarding preflight (W4); not yet wired in this phase.
#[allow(dead_code)]
pub fn existing_mcp_sources(roots: &ScanRoots) -> Vec<(&'static str, PathBuf)> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for src in MCP_CATALOG {
        for loc in src.locations {
            if !location_applies(loc, roots.os) {
                continue;
            }
            let Some(path) = resolve_location(loc, roots) else {
                continue;
            };
            if path.is_file() && seen.insert(path.clone()) {
                out.push((src.client, path));
            }
        }
    }
    out
}

/// Presence-only: returns paths for every instruction file (home + cwd,
/// including .cursor/rules/*) that exists on disk.
/// Called by the onboarding preflight (W4); not yet wired in this phase.
#[allow(dead_code)]
pub fn existing_instruction_files(roots: &ScanRoots) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for rel in INSTRUCTION_HOME {
        paths.push(roots.home.join(rel));
    }
    for rel in INSTRUCTION_CWD {
        paths.push(roots.cwd.join(rel));
    }
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".cursor/rules")) {
        let mut rules: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        rules.sort();
        paths.extend(rules);
    }
    let mut seen: HashSet<PathBuf> = HashSet::new();
    paths
        .into_iter()
        .filter(|p| p.is_file() && seen.insert(p.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Path resolution helpers
// ---------------------------------------------------------------------------

/// Resolve a catalog location to a concrete path given roots, returning None
/// when the required root is absent (config or data not set).
pub fn resolve_location(loc: &Location, roots: &ScanRoots) -> Option<PathBuf> {
    match loc.base {
        Base::Home => Some(roots.home.join(loc.rel)),
        Base::Cwd => Some(roots.cwd.join(loc.rel)),
        Base::Config => roots.config.as_ref().map(|c| c.join(loc.rel)),
    }
}

/// Whether a location applies to the given OS.
fn location_applies(loc: &Location, os: ScanOs) -> bool {
    match loc.only {
        None => true,
        Some(list) => list.contains(&os),
    }
}

/// Show a path with the home prefix collapsed to `~`.
pub fn display_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Internal collectors
// ---------------------------------------------------------------------------

fn collect_mcp(
    roots: &ScanRoots,
    path_var: &str,
    provider: CoreProvider,
    warnings: &mut Vec<String>,
) -> Vec<McpFinding> {
    // Dedupe by resolved path: when cwd is the home directory the project and
    // home entries for the same file would otherwise both fire.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();

    for src in MCP_CATALOG {
        for loc in src.locations {
            if !location_applies(loc, roots.os) {
                continue;
            }
            let Some(path) = resolve_location(loc, roots) else {
                continue;
            };
            if !path.is_file() || !seen.insert(path.clone()) {
                continue;
            }
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warnings.push(format!("could not read {}: {e}", path.display()));
                    continue;
                }
            };
            let config = match src.shape {
                Shape::Auto => Some(text),
                Shape::ClaudeJson => match extract_claude_code_servers(&text) {
                    Ok(c) => c,
                    Err(e) => {
                        warnings.push(format!("could not parse {}: {e}", path.display()));
                        None
                    }
                },
                Shape::CodexToml => match codex_toml_to_mcp_json(&text) {
                    Ok(c) => c,
                    Err(e) => {
                        warnings.push(format!("could not parse {}: {e}", path.display()));
                        None
                    }
                },
            };
            let Some(config) = config else { continue };

            match mcp::analyze(&config, provider) {
                Ok(analysis) => {
                    let swaps = collect_swaps(&analysis, path_var);
                    out.push(McpFinding {
                        client: src.client,
                        path,
                        analysis,
                        swaps,
                    });
                }
                // Settings files routinely exist without an MCP section; that is
                // an expected skip, not a problem. Real parse failures still warn.
                Err(e) if e.contains("no MCP servers") || e.contains("no servers in it") => {}
                Err(e) => warnings.push(format!("could not analyze {}: {e}", path.display())),
            }
        }
    }
    out
}

pub fn collect_swaps(analysis: &McpAnalysis, path_var: &str) -> Vec<SwapInfo> {
    analysis
        .servers
        .iter()
        .filter(|s| s.savings_tokens > 0)
        .filter_map(|s| {
            let cli = s.cli_alternative.clone()?;
            let binary = first_binary(&cli)?.to_string();
            let installed = find_in_path(&binary, path_var);
            Some(SwapInfo {
                server: s.name.clone(),
                cli,
                install_hint: install_hint(&binary),
                binary,
                installed,
                savings_tokens: s.savings_tokens,
            })
        })
        .collect()
}

/// The binary a CLI-alternative string starts with ("bash + find/cat/rg"
/// probes for bash, "xcodebuild / xcrun simctl" probes for xcodebuild).
pub fn first_binary(cli: &str) -> Option<&str> {
    cli.split_whitespace().next()
}

/// Probe `path_var` entries for an executable named `bin`. No shelling out:
/// this is a metadata check, never an invocation.
pub fn find_in_path(bin: &str, path_var: &str) -> bool {
    if bin.is_empty() || path_var.is_empty() {
        return false;
    }
    std::env::split_paths(path_var)
        .any(|dir| !dir.as_os_str().is_empty() && is_executable(&dir.join(bin)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn install_hint(binary: &str) -> String {
    let hint = match binary {
        "gh" => "brew install gh",
        "glab" => "brew install glab",
        "psql" => "brew install libpq",
        "aws" => "brew install awscli",
        "kubectl" => "brew install kubernetes-cli",
        "supabase" => "brew install supabase/tap/supabase",
        "sentry-cli" => "brew install getsentry/tools/sentry-cli",
        "jira-cli" => "brew install ankitpokhrel/jira-cli/jira-cli",
        "neonctl" => "bunx neonctl",
        _ => return format!("install {binary}"),
    };
    hint.to_string()
}

/// Convert a Codex `config.toml` to the JSON `mcpServers` shape that
/// `mcp::analyze` understands. `Ok(None)` means the file has no servers.
pub fn codex_toml_to_mcp_json(text: &str) -> Result<Option<String>, String> {
    let value: toml::Value = toml::from_str(text).map_err(|e| format!("TOML parse error: {e}"))?;
    let Some(servers) = value.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Ok(None);
    };
    if servers.is_empty() {
        return Ok(None);
    }
    let mut map = serde_json::Map::with_capacity(servers.len());
    for (name, server) in servers {
        let json = serde_json::to_value(server).map_err(|e| e.to_string())?;
        map.insert(name.clone(), json);
    }
    Ok(Some(serde_json::json!({ "mcpServers": map }).to_string()))
}

/// Extract the `mcpServers` subtrees from `~/.claude.json` (top-level plus
/// every project entry), merged into one small config. First occurrence of a
/// server name wins. `Ok(None)` means no servers anywhere in the file.
pub fn extract_claude_code_servers(text: &str) -> Result<Option<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("JSON parse error: {e}"))?;
    let mut merged = serde_json::Map::new();
    if let Some(top) = value.get("mcpServers").and_then(|v| v.as_object()) {
        for (name, server) in top {
            merged.entry(name.clone()).or_insert_with(|| server.clone());
        }
    }
    if let Some(projects) = value.get("projects").and_then(|v| v.as_object()) {
        for project in projects.values() {
            if let Some(servers) = project.get("mcpServers").and_then(|v| v.as_object()) {
                for (name, server) in servers {
                    merged.entry(name.clone()).or_insert_with(|| server.clone());
                }
            }
        }
    }
    if merged.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::json!({ "mcpServers": merged }).to_string(),
    ))
}

fn collect_instructions(
    roots: &ScanRoots,
    deep: bool,
    warnings: &mut Vec<String>,
) -> Vec<InstructionFile> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for rel in INSTRUCTION_HOME {
        paths.push(roots.home.join(rel));
    }
    for rel in INSTRUCTION_CWD {
        paths.push(roots.cwd.join(rel));
    }
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".cursor/rules")) {
        let mut rules: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        rules.sort();
        paths.extend(rules);
    }

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        if !path.is_file() || !seen.insert(path.clone()) {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", path.display()));
                continue;
            }
        };
        // Exact o200k count, matching the reference tokenizer the audit and
        // viz commands use.
        let tokens = match tokenize::count(TokProvider::OpenAi, &text) {
            Ok(n) => n as u64,
            Err(e) => {
                warnings.push(format!("could not tokenize {}: {e}", path.display()));
                continue;
            }
        };
        let report = deep.then(|| {
            audit::audit(
                &text,
                &AuditOptions {
                    input_tokens: Some(tokens),
                    include_experimental: false,
                },
            )
        });
        out.push(InstructionFile {
            path,
            tokens,
            audit: report,
        });
    }
    out
}

fn collect_shell(home: &Path, warnings: &mut Vec<String>) -> Vec<ShellFinding> {
    let mut out = Vec::new();
    for rel in SHELL_CONFIGS {
        let path = home.join(rel);
        if !path.is_file() {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", path.display()));
                continue;
            }
        };
        let result = redact::redact(&text, &RedactOptions::default());
        // Privacy rule (absolute): keep only the count and the kind names.
        // The redacted text, values, and spans are dropped right here.
        let mut kinds: Vec<String> = Vec::new();
        let mut count = 0usize;
        for finding in &result.findings {
            if !finding.redacted {
                continue;
            }
            count += 1;
            if !kinds.contains(&finding.kind) {
                kinds.push(finding.kind.clone());
            }
        }
        out.push(ShellFinding {
            path,
            secret_count: count,
            kinds,
        });
    }
    out
}

fn collect_environment(cwd: &Path, path_var: &str) -> Vec<String> {
    let mut out: Vec<String> = ENV_BINARIES
        .iter()
        .filter(|bin| find_in_path(bin, path_var))
        .map(|bin| (*bin).to_string())
        .collect();
    if let Ok(pin) = fs::read_to_string(cwd.join(".nvmrc")) {
        let pin = pin.trim();
        if !pin.is_empty() {
            out.push(format!("node {pin} pinned via .nvmrc"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-scan-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Build a ScanRoots with explicit injected directories (no dirs:: calls).
    fn make_roots(home: PathBuf, cwd: PathBuf, config: PathBuf, os: ScanOs) -> ScanRoots {
        ScanRoots {
            home,
            cwd,
            config: Some(config),
            data: None,
            os,
        }
    }

    #[test]
    fn codex_toml_converts_to_mcp_servers_shape() {
        let text = r#"
model = "gpt-5.4"

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.github.env]
GITHUB_TOKEN = "placeholder"
"#;
        let json = codex_toml_to_mcp_json(text).unwrap().unwrap();
        let analysis = mcp::analyze(&json, CoreProvider::Anthropic).unwrap();
        assert_eq!(analysis.servers.len(), 1);
        assert_eq!(analysis.servers[0].matched_id.as_deref(), Some("github"));

        assert!(codex_toml_to_mcp_json("model = \"x\"").unwrap().is_none());
        assert!(codex_toml_to_mcp_json("[mcp_servers]").unwrap().is_none());
        assert!(codex_toml_to_mcp_json("not toml [").is_err());
    }

    #[test]
    fn claude_json_extracts_top_level_and_project_servers() {
        let text = r#"{
          "numStartups": 412,
          "mcpServers": {
            "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] }
          },
          "projects": {
            "/Users/x/proj-a": {
              "history": ["irrelevant"],
              "mcpServers": {
                "linear": { "url": "https://mcp.linear.app/sse" },
                "github": { "command": "different-but-duplicate" }
              }
            },
            "/Users/x/proj-b": {}
          }
        }"#;
        let config = extract_claude_code_servers(text).unwrap().unwrap();
        let analysis = mcp::analyze(&config, CoreProvider::Anthropic).unwrap();
        // github deduped across top level and project, linear picked up.
        assert_eq!(analysis.servers.len(), 2);
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["mcpServers"]["github"]["command"], "npx",
            "first occurrence wins"
        );

        assert!(extract_claude_code_servers("{}").unwrap().is_none());
        assert!(extract_claude_code_servers("not json").is_err());
    }

    #[test]
    fn path_probe_finds_executables_without_shelling_out() {
        let dir = tmp("probe");
        let bin = dir.join("gh");
        fs::write(&bin, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path_var = dir.display().to_string();
        assert!(find_in_path("gh", &path_var));
        assert!(!find_in_path("psql", &path_var));
        assert!(!find_in_path("gh", ""));
        assert!(!find_in_path("", &path_var));
    }

    #[cfg(unix)]
    #[test]
    fn path_probe_ignores_non_executable_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("probe-noexec");
        let file = dir.join("notabinary");
        fs::write(&file, "data").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!find_in_path("notabinary", &dir.display().to_string()));
    }

    #[test]
    fn discovery_finds_configs_and_empty_scan_is_calm() {
        let home = tmp("home");
        let cwd = tmp("cwd");
        let config = tmp("config");
        let roots = make_roots(home.clone(), cwd.clone(), config.clone(), ScanOs::MacOs);

        let empty = scan(&roots, "", CoreProvider::Anthropic, false);
        assert!(empty.mcp.is_empty());
        assert!(empty.instruction_files.is_empty());
        assert!(empty.shell.is_empty());
        assert!(empty.environment.is_empty());
        assert!(empty.warnings.is_empty(), "{:?}", empty.warnings);

        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::write(
            home.join(".cursor/mcp.json"),
            r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#,
        )
        .unwrap();
        fs::write(cwd.join("CLAUDE.md"), "# Project rules\n\nUse bun.\n").unwrap();

        let roots2 = make_roots(home, cwd, config, ScanOs::MacOs);
        let report = scan(&roots2, "", CoreProvider::Anthropic, false);
        assert_eq!(report.mcp.len(), 1);
        assert_eq!(report.mcp[0].client, "Cursor");
        assert_eq!(report.mcp[0].analysis.totals.servers, 1);
        assert_eq!(report.instruction_files.len(), 1);
        assert!(report.instruction_files[0].tokens > 0);
        assert!(report.instruction_files[0].audit.is_none(), "not deep");
    }

    #[test]
    fn swaps_annotate_installed_state() {
        let dir = tmp("swaps");
        let gh = dir.join("gh");
        fs::write(&gh, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let config = r#"{ "mcpServers": {
            "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] },
            "postgres": { "command": "npx", "args": ["@modelcontextprotocol/server-postgres"] }
        } }"#;
        let analysis = mcp::analyze(config, CoreProvider::Anthropic).unwrap();
        let swaps = collect_swaps(&analysis, &dir.display().to_string());
        assert_eq!(swaps.len(), 2);
        let gh_swap = swaps.iter().find(|s| s.binary == "gh").unwrap();
        assert!(gh_swap.installed);
        let psql_swap = swaps.iter().find(|s| s.binary == "psql").unwrap();
        assert!(!psql_swap.installed);
        assert_eq!(psql_swap.install_hint, "brew install libpq");
    }

    #[test]
    fn first_binary_takes_the_leading_token() {
        assert_eq!(first_binary("gh"), Some("gh"));
        assert_eq!(first_binary("bash + find/cat/rg"), Some("bash"));
        assert_eq!(
            first_binary("xcodebuild / xcrun simctl"),
            Some("xcodebuild")
        );
        assert_eq!(first_binary(""), None);
    }

    #[test]
    fn shell_findings_carry_kinds_and_counts_only() {
        let home = tmp("shell-home");
        fs::write(
            home.join(".zshrc"),
            "export PATH=$PATH:/opt/bin\nexport OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwx123456\n",
        )
        .unwrap();
        let mut warnings = Vec::new();
        let shell = collect_shell(&home, &mut warnings);
        assert_eq!(shell.len(), 1);
        assert!(shell[0].secret_count >= 1);
        assert!(shell[0]
            .kinds
            .iter()
            .any(|k| k.contains("key") || k.contains("secret") || k.contains("entropy")));
        // The struct has no field that could carry a value or span; assert the
        // kinds list does not contain the secret itself as a belt-and-braces check.
        assert!(shell[0].kinds.iter().all(|k| !k.contains("sk-proj")));
    }

    // -----------------------------------------------------------------------
    // Per-OS resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn claude_desktop_found_via_config_dir_on_all_three_oses() {
        for os in [ScanOs::MacOs, ScanOs::Linux, ScanOs::Windows] {
            let home = tmp(&format!("cd-home-{os:?}"));
            let cwd = tmp(&format!("cd-cwd-{os:?}"));
            let config = tmp(&format!("cd-config-{os:?}"));

            // Write the config file under the injected config dir.
            let cfg_dir = config.join("Claude");
            fs::create_dir_all(&cfg_dir).unwrap();
            fs::write(
                cfg_dir.join("claude_desktop_config.json"),
                r#"{ "mcpServers": { "test": { "command": "echo" } } }"#,
            )
            .unwrap();

            let roots = make_roots(home, cwd, config, os);
            let found = existing_mcp_sources(&roots);
            let clients: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
            assert!(
                clients.contains(&"Claude Desktop"),
                "os={os:?}: expected Claude Desktop in {clients:?}"
            );
        }
    }

    #[test]
    fn vscode_found_via_config_dir_on_all_three_oses() {
        for os in [ScanOs::MacOs, ScanOs::Linux, ScanOs::Windows] {
            let home = tmp(&format!("vsc-home-{os:?}"));
            let cwd = tmp(&format!("vsc-cwd-{os:?}"));
            let config = tmp(&format!("vsc-config-{os:?}"));

            let cfg_dir = config.join("Code").join("User");
            fs::create_dir_all(&cfg_dir).unwrap();
            fs::write(
                cfg_dir.join("settings.json"),
                r#"{ "mcp": { "servers": { "test": { "command": "echo" } } } }"#,
            )
            .unwrap();

            let roots = make_roots(home, cwd, config, os);
            let found = existing_mcp_sources(&roots);
            let clients: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
            assert!(
                clients.contains(&"VS Code"),
                "os={os:?}: expected VS Code in {clients:?}"
            );
        }
    }

    #[test]
    fn zed_divergence_mac_linux_use_home_config_windows_uses_config_dir() {
        // macOS and Linux: ~/.config/zed/settings.json
        for os in [ScanOs::MacOs, ScanOs::Linux] {
            let home = tmp(&format!("zed-home-{os:?}"));
            let cwd = tmp(&format!("zed-cwd-{os:?}"));
            let config = tmp(&format!("zed-config-{os:?}"));

            let zed_home_dir = home.join(".config").join("zed");
            fs::create_dir_all(&zed_home_dir).unwrap();
            fs::write(zed_home_dir.join("settings.json"), r#"{ "lsp": {} }"#).unwrap();

            let roots = make_roots(home.clone(), cwd, config.clone(), os);
            // Verify path resolution: the active Zed location must point under
            // home (not config dir) on macOS and Linux.
            let zed_src = MCP_CATALOG.iter().find(|s| s.client == "Zed").unwrap();
            let paths: Vec<PathBuf> = zed_src
                .locations
                .iter()
                .filter(|l| location_applies(l, os))
                .filter_map(|l| resolve_location(l, &roots))
                .collect();
            assert_eq!(
                paths.len(),
                1,
                "os={os:?}: expected one active Zed location"
            );
            assert!(
                paths[0].starts_with(&home),
                "os={os:?}: Zed path should be under home, got {:?}",
                paths[0]
            );
            assert!(
                !paths[0].starts_with(&config),
                "os={os:?}: Zed path must not be under config dir on mac/linux"
            );
            // existing_mcp_sources is presence-only: the file exists so Zed
            // appears regardless of whether it has MCP servers.
            let found = existing_mcp_sources(&roots);
            let clients: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
            assert!(
                clients.contains(&"Zed"),
                "os={os:?}: Zed file exists so it should appear in mcp_sources: {clients:?}"
            );
        }

        // Windows: config_dir/Zed/settings.json
        {
            let home = tmp("zed-home-Windows");
            let cwd = tmp("zed-cwd-Windows");
            let config = tmp("zed-config-Windows");

            let zed_win_dir = config.join("Zed");
            fs::create_dir_all(&zed_win_dir).unwrap();
            fs::write(zed_win_dir.join("settings.json"), r#"{ "lsp": {} }"#).unwrap();

            let roots = make_roots(home.clone(), cwd, config.clone(), ScanOs::Windows);
            let zed_src = MCP_CATALOG.iter().find(|s| s.client == "Zed").unwrap();
            let paths: Vec<PathBuf> = zed_src
                .locations
                .iter()
                .filter(|l| location_applies(l, ScanOs::Windows))
                .filter_map(|l| resolve_location(l, &roots))
                .collect();
            assert_eq!(paths.len(), 1, "Windows: expected one active Zed location");
            assert!(
                paths[0].starts_with(&config),
                "Windows: Zed path should be under config dir, got {:?}",
                paths[0]
            );
            assert!(
                !paths[0].starts_with(&home),
                "Windows: Zed path must not be under home"
            );
        }
    }

    #[test]
    fn zed_home_config_path_not_active_when_os_is_windows() {
        let home = tmp("zed-win-home-check");
        let cwd = tmp("zed-win-cwd-check");
        let config = tmp("zed-win-config-check");

        // Write ONLY the home-style path; windows should not pick it up.
        let zed_home_dir = home.join(".config").join("zed");
        fs::create_dir_all(&zed_home_dir).unwrap();
        fs::write(
            zed_home_dir.join("settings.json"),
            r#"{ "mcpServers": { "test": { "command": "echo" } } }"#,
        )
        .unwrap();

        let roots = make_roots(home, cwd, config, ScanOs::Windows);
        let found = existing_mcp_sources(&roots);
        let clients: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
        assert!(
            !clients.contains(&"Zed"),
            "Windows: Zed must not be found at home/.config/zed path, got {clients:?}"
        );
    }

    #[test]
    fn existing_mcp_sources_returns_presence_only_results() {
        let home = tmp("ems-home");
        let cwd = tmp("ems-cwd");
        let config = tmp("ems-config");

        // Write two sources.
        let cursor_dir = home.join(".cursor");
        fs::create_dir_all(&cursor_dir).unwrap();
        fs::write(
            cursor_dir.join("mcp.json"),
            r#"{ "mcpServers": { "gh": { "command": "gh" } } }"#,
        )
        .unwrap();

        let claude_dir = config.join("Claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("claude_desktop_config.json"),
            r#"{ "mcpServers": { "pg": { "command": "psql" } } }"#,
        )
        .unwrap();

        let roots = make_roots(home.clone(), cwd, config.clone(), ScanOs::MacOs);
        let found = existing_mcp_sources(&roots);

        // Both are found.
        let clients: Vec<&str> = found.iter().map(|(c, _)| *c).collect();
        assert!(clients.contains(&"Cursor"), "Cursor missing: {clients:?}");
        assert!(
            clients.contains(&"Claude Desktop"),
            "Claude Desktop missing: {clients:?}"
        );

        // Paths are the resolved absolute paths, not relative strings.
        for (_, path) in &found {
            assert!(path.is_absolute(), "path must be absolute: {path:?}");
        }

        // No duplicates (dedup by path).
        let paths: Vec<&PathBuf> = found.iter().map(|(_, p)| p).collect();
        let unique: HashSet<&&PathBuf> = paths.iter().collect();
        assert_eq!(paths.len(), unique.len(), "duplicate paths in result");
    }

    #[test]
    fn existing_instruction_files_returns_presence_only_results() {
        let home = tmp("eif-home");
        let cwd = tmp("eif-cwd");
        let config = tmp("eif-config");

        // Write fixtures.
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("CLAUDE.md"), "# Global rules\n").unwrap();
        fs::write(cwd.join("AGENTS.md"), "# Agent rules\n").unwrap();

        let cursor_rules = cwd.join(".cursor").join("rules");
        fs::create_dir_all(&cursor_rules).unwrap();
        fs::write(cursor_rules.join("style.md"), "use tabs\n").unwrap();
        fs::write(cursor_rules.join("testing.md"), "write tests\n").unwrap();

        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let files = existing_instruction_files(&roots);

        // Expect .claude/CLAUDE.md, AGENTS.md, and the two cursor rules.
        assert_eq!(
            files.len(),
            4,
            "expected 4 instruction files, got: {files:?}"
        );

        for path in &files {
            assert!(path.is_absolute(), "path must be absolute: {path:?}");
            assert!(path.is_file(), "each path must exist: {path:?}");
        }

        // No duplicates.
        let unique: HashSet<&PathBuf> = files.iter().collect();
        assert_eq!(files.len(), unique.len(), "duplicate paths in result");
    }
}
