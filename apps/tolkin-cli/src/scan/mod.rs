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
// Per-file (or per-directory) instruction surfaces loaded into agent context.
//
// EXCLUDED DELIBERATELY:
//   - promptfoo.yaml: configures eval harnesses, not loaded into agent context.
//   - litellm.yaml: configures an LLM proxy, not loaded into agent context.
// Counting those files would mislead users about what tokens their agent sees.
// The workflows detector (item c) handles .github/workflows prompts separately
// because workflow prompts load per-CI-run, not per-agent-session.
const INSTRUCTION_CWD: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".cursorrules",
    ".clinerules",
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

/// Hooks configuration detected in a `.claude/settings.json` or
/// `.claude/settings.local.json` file (project or user level). Detection only:
/// we check for the presence of the `hooks` key and which event names are
/// configured. We never parse hook commands, scripts, or matchers beyond the
/// event-name set; those are runtime details that do not belong in a static scan.
#[derive(Debug)]
pub struct HooksFinding {
    pub path: std::path::PathBuf,
    /// The scope label: "user" for the user-level settings file, "project" for
    /// the project-level one.
    pub scope: &'static str,
    /// Event names configured under the `hooks` key, sorted. Empty when the
    /// `hooks` key is present but empty. Absent events are not listed.
    pub events: Vec<String>,
}

/// One workflow file containing LLM-invoking steps (uses: anthropics/claude-code-action
/// variants, or prompt-bearing fields like prompt:, system_prompt:, claude_args:).
/// Token counts cover ONLY the prompt-bearing string values, never the whole
/// workflow file. Workflow prompts load per-CI-run, not per-agent-session, so
/// these numbers are reported separately and NEVER folded into always-loaded totals.
#[derive(Debug)]
pub struct WorkflowLlmFinding {
    pub path: PathBuf,
    /// Prompt-bearing field values found in this workflow, with the field name.
    pub prompt_fields: Vec<WorkflowPromptField>,
    /// Sum of o200k token counts for all prompt-bearing field values.
    pub prompt_tokens: u64,
}

/// One prompt-bearing field value extracted from a workflow step.
#[derive(Debug)]
pub struct WorkflowPromptField {
    /// The YAML field name, e.g. "prompt", "system_prompt", "claude_args".
    pub field: String,
    /// Token count of the field value.
    pub tokens: u64,
}

#[derive(Debug)]
pub struct ScanReport {
    pub mcp: Vec<McpFinding>,
    pub instruction_files: Vec<InstructionFile>,
    pub shell: Vec<ShellFinding>,
    /// LLM-invoking workflow steps. Prompt tokens are per-CI-run, not
    /// per-agent-session, and are NEVER included in always-loaded totals.
    pub workflows_llm: Vec<WorkflowLlmFinding>,
    /// Claude Code hooks configuration detected in settings files. Empty means
    /// no hooks key was found anywhere; one entry per settings file that carries
    /// the key (at most two: user-level and project-level).
    pub hooks: Vec<HooksFinding>,
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
    let workflows_llm = collect_workflows_llm(&roots.cwd, &mut warnings);
    let hooks = collect_hooks(roots, &mut warnings);
    let environment = collect_environment(&roots.cwd, path_var);
    ScanReport {
        mcp,
        instruction_files,
        shell,
        workflows_llm,
        hooks,
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
/// including .cursor/rules/* and .windsurf/rules/*) that exists on disk.
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
    // .cursor/rules/ directory: each file is a separate rule loaded into context.
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".cursor/rules")) {
        let mut rules: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        rules.sort();
        paths.extend(rules);
    }
    // .windsurf/rules/ directory: each file is a separate Windsurf rule loaded
    // into context (distinct from ~/.codeium/windsurf/mcp_config.json which
    // is an MCP config, not an instruction surface).
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".windsurf/rules")) {
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
    // .cursor/rules/ directory: each file is a separate rule loaded into context.
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".cursor/rules")) {
        let mut rules: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        rules.sort();
        paths.extend(rules);
    }
    // .windsurf/rules/ directory: each file is a separate Windsurf rule loaded
    // into context. Distinct from ~/.codeium/windsurf/mcp_config.json (MCP).
    if let Ok(entries) = fs::read_dir(roots.cwd.join(".windsurf/rules")) {
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

/// Detect Claude Code hooks configuration in settings files.
///
/// Checked locations (presence-only; never parsed beyond the `hooks` key):
/// - Project-level: `<cwd>/.claude/settings.json` and `<cwd>/.claude/settings.local.json`
/// - User-level: `<home>/.claude/settings.json`
///
/// Detection is tolerant of malformed JSON: a file that cannot be parsed
/// contributes no finding (silently skipped; a warning is added so the caller
/// can disclose the skip). The `hooks` value is parsed only enough to extract
/// the set of event-name keys; no deeper structure is read.
fn collect_hooks(roots: &ScanRoots, warnings: &mut Vec<String>) -> Vec<HooksFinding> {
    let candidates: &[(&std::path::Path, &'static str)] =
        &[(&roots.home, "user"), (&roots.cwd, "project")];
    let rel_paths = [".claude/settings.json", ".claude/settings.local.json"];
    let mut out: Vec<HooksFinding> = Vec::new();

    for (base, scope) in candidates {
        for rel in &rel_paths {
            let path = base.join(rel);
            if !path.is_file() {
                continue;
            }
            let text = match fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    warnings.push(format!(
                        "hooks scan: could not read {}: {e}",
                        path.display()
                    ));
                    continue;
                }
            };
            // Tolerant parse: malformed JSON means no hooks finding for this file.
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(hooks_val) = value.get("hooks") else {
                continue;
            };
            // Extract event names: the hooks value is expected to be an object
            // whose keys are event names. Anything that is not an object yields
            // an empty event list (presence still recorded).
            let mut events: Vec<String> = hooks_val
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            events.sort();
            out.push(HooksFinding {
                path,
                scope,
                events,
            });
        }
    }
    out
}

/// Action prefixes / names that indicate an LLM-invoking step.
/// Matched case-insensitively against the `uses:` value.
const LLM_ACTION_MATCHERS: &[&str] = &["anthropics/claude-code-action", "anthropics/claude-action"];

/// Prompt-bearing YAML field names checked under each step.
const PROMPT_FIELDS: &[&str] = &["prompt", "system_prompt", "claude_args"];

/// Scan `.github/workflows/` for files containing LLM-invoking steps and
/// count the tokens of prompt-bearing field values only. Returns one entry
/// per file that has at least one LLM step with a non-empty prompt field.
///
/// Uses a line-by-line YAML heuristic rather than a full YAML parse to keep
/// the dependency footprint minimal and match the scan module's posture of
/// no external spawns. The heuristic is conservative: it looks for `uses:`
/// lines containing known action prefixes and, once inside such a step, for
/// `prompt:`, `system_prompt:`, or `claude_args:` fields on the same
/// indentation level.
fn collect_workflows_llm(cwd: &Path, warnings: &mut Vec<String>) -> Vec<WorkflowLlmFinding> {
    let dir = cwd.join(".github").join("workflows");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "yml" || e == "yaml")
        })
        .collect();
    files.sort();

    for path in files {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", path.display()));
                continue;
            }
        };
        let finding = scan_workflow_llm_steps(&path, &text, warnings);
        if let Some(f) = finding {
            out.push(f);
        }
    }
    out
}

/// Parse one workflow file and return a finding if it contains LLM-invoking
/// steps with prompt-bearing fields.
///
/// Uses a two-pass heuristic:
///
/// Pass 1: check whether the file contains any LLM action at all (quick
/// substring scan so non-LLM workflows exit fast).
///
/// Pass 2: extract prompt-bearing field values. The YAML structure for a
/// GitHub Actions `with:` block looks like:
///
/// ```yaml
///   - uses: anthropics/claude-code-action@v1
///     with:
///       prompt: Some prompt text.
///       system_prompt: |
///         Multi-line
///         value here.
/// ```
///
/// The heuristic looks for the `uses:` line (or `- uses:` list item) that
/// contains an LLM action name, then collects any `prompt:`,
/// `system_prompt:`, or `claude_args:` key that appears at a deeper
/// indentation level within the same step. The step boundary is detected
/// by a line that starts a new list item (`- `) at the same or shallower
/// indentation as the `uses:` line.
fn scan_workflow_llm_steps(
    path: &Path,
    text: &str,
    _warnings: &mut Vec<String>,
) -> Option<WorkflowLlmFinding> {
    // Quick gate: skip files with no LLM action reference at all.
    let lower = text.to_ascii_lowercase();
    let has_llm_action = LLM_ACTION_MATCHERS.iter().any(|m| lower.contains(m));
    if !has_llm_action {
        return None;
    }

    let mut prompt_fields: Vec<WorkflowPromptField> = Vec::new();

    #[derive(PartialEq)]
    enum State {
        Scanning,
        /// We found a `uses:` line with an LLM action; now collect prompt
        /// fields that appear at a deeper indent than `step_indent`. The
        /// step ends when we see a new list item at or below `step_indent`.
        InLlmStep {
            step_indent: usize,
        },
        /// Inside a block-scalar value for a prompt field.
        InBlockScalar {
            field: String,
            value: String,
            /// Indent level of the field key line (block content is deeper).
            field_indent: usize,
            step_indent: usize,
        },
    }

    let mut state = State::Scanning;

    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let trimmed = trimmed.trim_end();

        match &state {
            State::Scanning => {
                // Match both `uses: <action>` and `- uses: <action>`.
                let uses_value = if let Some(rest) = trimmed.strip_prefix("- uses:") {
                    Some(rest)
                } else {
                    trimmed.strip_prefix("uses:")
                };
                if let Some(rest) = uses_value {
                    let action = rest.trim().trim_matches('\'').trim_matches('"');
                    let action_lower = action.to_ascii_lowercase();
                    if LLM_ACTION_MATCHERS.iter().any(|m| action_lower.contains(m)) {
                        // The step indent is the indent of the `- uses:` or
                        // `uses:` line.
                        state = State::InLlmStep {
                            step_indent: indent,
                        };
                    }
                }
            }
            State::InLlmStep { step_indent } => {
                let step_indent = *step_indent;
                // A new list item at or shallower than step_indent ends this step.
                if !trimmed.is_empty() && indent <= step_indent && trimmed.starts_with("- ") {
                    // New step. Check if this new step is also an LLM step.
                    let uses_value = trimmed.strip_prefix("- uses:");
                    if let Some(rest) = uses_value {
                        let action = rest.trim().trim_matches('\'').trim_matches('"');
                        let action_lower = action.to_ascii_lowercase();
                        if LLM_ACTION_MATCHERS.iter().any(|m| action_lower.contains(m)) {
                            state = State::InLlmStep {
                                step_indent: indent,
                            };
                        } else {
                            state = State::Scanning;
                        }
                    } else {
                        state = State::Scanning;
                    }
                    continue;
                }
                // Check for prompt-bearing fields at deeper indent.
                if indent > step_indent {
                    for &field in PROMPT_FIELDS {
                        let prefix = format!("{field}:");
                        if trimmed.starts_with(&prefix) {
                            let value_part = trimmed.strip_prefix(&prefix).unwrap_or("").trim();
                            if value_part == "|" || value_part == "|-" || value_part == "|+" {
                                state = State::InBlockScalar {
                                    field: field.to_string(),
                                    value: String::new(),
                                    field_indent: indent,
                                    step_indent,
                                };
                            } else if !value_part.is_empty() {
                                let v = value_part.trim_matches('\'').trim_matches('"').to_string();
                                let tokens =
                                    tokenize::count(TokProvider::OpenAi, &v).unwrap_or(0) as u64;
                                if tokens > 0 {
                                    prompt_fields.push(WorkflowPromptField {
                                        field: field.to_string(),
                                        tokens,
                                    });
                                }
                            }
                            break;
                        }
                    }
                }
            }
            State::InBlockScalar {
                field,
                value,
                field_indent,
                step_indent,
            } => {
                // Block scalar ends when a non-empty line returns to or below
                // field_indent.
                if !trimmed.is_empty() && indent <= *field_indent {
                    let v = value.clone();
                    let tokens = tokenize::count(TokProvider::OpenAi, &v).unwrap_or(0) as u64;
                    if tokens > 0 {
                        prompt_fields.push(WorkflowPromptField {
                            field: field.clone(),
                            tokens,
                        });
                    }
                    // Re-enter InLlmStep to keep scanning (or exit to Scanning).
                    let si = *step_indent;
                    let fi = *field_indent;
                    if !trimmed.is_empty() && indent <= si && trimmed.starts_with("- ") {
                        // New step.
                        let uses_value = trimmed.strip_prefix("- uses:");
                        if let Some(rest) = uses_value {
                            let action = rest.trim().trim_matches('\'').trim_matches('"');
                            let action_lower = action.to_ascii_lowercase();
                            if LLM_ACTION_MATCHERS.iter().any(|m| action_lower.contains(m)) {
                                state = State::InLlmStep {
                                    step_indent: indent,
                                };
                            } else {
                                state = State::Scanning;
                            }
                        } else {
                            state = State::Scanning;
                        }
                    } else if indent > si {
                        // Still inside the step; check for more prompt fields.
                        state = State::InLlmStep { step_indent: si };
                        // Process this line as InLlmStep.
                        for &f2 in PROMPT_FIELDS {
                            let prefix = format!("{f2}:");
                            if trimmed.starts_with(&prefix) {
                                let value_part = trimmed.strip_prefix(&prefix).unwrap_or("").trim();
                                if value_part == "|" || value_part == "|-" || value_part == "|+" {
                                    state = State::InBlockScalar {
                                        field: f2.to_string(),
                                        value: String::new(),
                                        field_indent: fi,
                                        step_indent: si,
                                    };
                                } else if !value_part.is_empty() {
                                    let v =
                                        value_part.trim_matches('\'').trim_matches('"').to_string();
                                    let t = tokenize::count(TokProvider::OpenAi, &v).unwrap_or(0)
                                        as u64;
                                    if t > 0 {
                                        prompt_fields.push(WorkflowPromptField {
                                            field: f2.to_string(),
                                            tokens: t,
                                        });
                                    }
                                }
                                break;
                            }
                        }
                    } else {
                        state = State::Scanning;
                    }
                } else if !trimmed.is_empty() {
                    // Still inside the block scalar.
                    let v = value.clone() + if value.is_empty() { "" } else { "\n" } + trimmed;
                    if let State::InBlockScalar { value, .. } = &mut state {
                        *value = v;
                    }
                }
            }
        }
    }

    // Flush any in-progress block scalar at EOF.
    if let State::InBlockScalar { field, value, .. } = &state {
        let tokens = tokenize::count(TokProvider::OpenAi, value).unwrap_or(0) as u64;
        if tokens > 0 {
            prompt_fields.push(WorkflowPromptField {
                field: field.clone(),
                tokens,
            });
        }
    }

    if prompt_fields.is_empty() {
        return None;
    }
    let prompt_tokens: u64 = prompt_fields.iter().map(|f| f.tokens).sum();
    Some(WorkflowLlmFinding {
        path: path.to_path_buf(),
        prompt_fields,
        prompt_tokens,
    })
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

    #[test]
    fn windsurf_rules_dir_is_discovered() {
        let home = tmp("ws-rules-home");
        let cwd = tmp("ws-rules-cwd");
        let config = tmp("ws-rules-config");

        let windsurf_rules = cwd.join(".windsurf").join("rules");
        fs::create_dir_all(&windsurf_rules).unwrap();
        fs::write(windsurf_rules.join("coding.md"), "write tests\n").unwrap();
        fs::write(windsurf_rules.join("style.md"), "use snake_case\n").unwrap();

        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let files = existing_instruction_files(&roots);

        // Both windsurf rule files appear.
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"coding.md".to_string()),
            "coding.md missing: {names:?}"
        );
        assert!(
            names.contains(&"style.md".to_string()),
            "style.md missing: {names:?}"
        );

        // All paths are absolute and exist.
        for path in &files {
            assert!(path.is_absolute(), "not absolute: {path:?}");
            assert!(path.is_file(), "not a file: {path:?}");
        }
    }

    #[test]
    fn windsurf_rules_dir_discovered_on_linux() {
        let home = tmp("ws-linux-home");
        let cwd = tmp("ws-linux-cwd");
        let config = tmp("ws-linux-config");

        let windsurf_rules = cwd.join(".windsurf").join("rules");
        fs::create_dir_all(&windsurf_rules).unwrap();
        fs::write(windsurf_rules.join("rules.md"), "be concise\n").unwrap();

        // Linux roots, same path resolution.
        let roots = make_roots(home, cwd, config, ScanOs::Linux);
        let files = existing_instruction_files(&roots);

        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"rules.md".to_string()),
            "rules.md missing: {names:?}"
        );
    }

    #[test]
    fn windsurf_rules_dir_discovered_on_windows() {
        let home = tmp("ws-win-home");
        let cwd = tmp("ws-win-cwd");
        let config = tmp("ws-win-config");

        let windsurf_rules = cwd.join(".windsurf").join("rules");
        fs::create_dir_all(&windsurf_rules).unwrap();
        fs::write(windsurf_rules.join("win-rule.md"), "windows rule\n").unwrap();

        let roots = make_roots(home, cwd, config, ScanOs::Windows);
        let files = existing_instruction_files(&roots);

        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"win-rule.md".to_string()),
            "win-rule.md missing: {names:?}"
        );
    }

    #[test]
    fn clinerules_file_is_discovered() {
        let home = tmp("cline-home");
        let cwd = tmp("cline-cwd");
        let config = tmp("cline-config");

        fs::write(cwd.join(".clinerules"), "use structured output\n").unwrap();

        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let files = existing_instruction_files(&roots);

        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&".clinerules".to_string()),
            ".clinerules missing: {names:?}"
        );
    }

    #[test]
    fn clinerules_discovered_on_all_platforms() {
        for os in [ScanOs::MacOs, ScanOs::Linux, ScanOs::Windows] {
            let label = format!("{os:?}");
            let home = tmp(&format!("cline-plat-home-{label}"));
            let cwd = tmp(&format!("cline-plat-cwd-{label}"));
            let config = tmp(&format!("cline-plat-config-{label}"));

            fs::write(cwd.join(".clinerules"), "platform rule\n").unwrap();

            let roots = make_roots(home, cwd, config, os);
            let files = existing_instruction_files(&roots);

            let names: Vec<String> = files
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            assert!(
                names.contains(&".clinerules".to_string()),
                ".clinerules missing on {label}: {names:?}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Workflow LLM detector tests
    // ---------------------------------------------------------------------------

    fn make_workflow_dir(cwd: &Path) -> PathBuf {
        let dir = cwd.join(".github").join("workflows");
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn workflow_with_claude_action_and_inline_prompt_is_detected() {
        let cwd = tmp("wf-inline");
        let dir = make_workflow_dir(&cwd);
        fs::write(
            dir.join("review.yml"),
            r#"
on: [pull_request]
jobs:
  review:
    steps:
      - uses: anthropics/claude-code-action@v1
        with:
          prompt: Review this pull request for correctness.
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let findings = collect_workflows_llm(&cwd, &mut warnings);
        assert_eq!(findings.len(), 1, "expected 1 workflow finding");
        assert!(
            findings[0].prompt_tokens > 0,
            "expected non-zero prompt tokens"
        );
        let fields: Vec<&str> = findings[0]
            .prompt_fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert!(
            fields.contains(&"prompt"),
            "expected 'prompt' field: {fields:?}"
        );
    }

    #[test]
    fn workflow_with_system_prompt_field_is_detected() {
        let cwd = tmp("wf-sysprompt");
        let dir = make_workflow_dir(&cwd);
        fs::write(
            dir.join("agent.yml"),
            r#"
on: [push]
jobs:
  agent:
    steps:
      - uses: anthropics/claude-code-action@v2
        with:
          system_prompt: You are a helpful code reviewer.
          prompt: Summarize the changes.
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let findings = collect_workflows_llm(&cwd, &mut warnings);
        assert_eq!(findings.len(), 1);
        let fields: Vec<&str> = findings[0]
            .prompt_fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert!(
            fields.contains(&"system_prompt"),
            "system_prompt missing: {fields:?}"
        );
        assert!(fields.contains(&"prompt"), "prompt missing: {fields:?}");
    }

    #[test]
    fn workflow_without_llm_action_is_not_detected() {
        let cwd = tmp("wf-no-llm");
        let dir = make_workflow_dir(&cwd);
        fs::write(
            dir.join("ci.yml"),
            r#"
on: [push]
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let findings = collect_workflows_llm(&cwd, &mut warnings);
        assert!(
            findings.is_empty(),
            "expected no findings for non-LLM workflow"
        );
    }

    #[test]
    fn workflow_without_github_dir_returns_empty() {
        let cwd = tmp("wf-no-dir");
        // No .github/workflows/ directory.
        let mut warnings = Vec::new();
        let findings = collect_workflows_llm(&cwd, &mut warnings);
        assert!(findings.is_empty(), "expected empty when no workflows dir");
    }

    #[test]
    fn workflow_prompt_tokens_not_zero_for_real_text() {
        let cwd = tmp("wf-tokens");
        let dir = make_workflow_dir(&cwd);
        fs::write(
            dir.join("prompt.yml"),
            r#"
on: [pull_request]
jobs:
  pr_review:
    steps:
      - uses: anthropics/claude-code-action@beta
        with:
          prompt: Please review this pull request carefully and check for any correctness bugs, style issues, and missing tests.
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let findings = collect_workflows_llm(&cwd, &mut warnings);
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].prompt_tokens >= 10,
            "expected at least 10 tokens for the prompt text"
        );
    }

    // ---------------------------------------------------------------------------
    // Hooks detector tests
    // ---------------------------------------------------------------------------

    #[test]
    fn hooks_absent_when_no_settings_files_exist() {
        let home = tmp("hooks-absent-home");
        let cwd = tmp("hooks-absent-cwd");
        let config = tmp("hooks-absent-config");
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert!(findings.is_empty(), "expected no hooks: {findings:?}");
        assert!(warnings.is_empty());
    }

    #[test]
    fn hooks_absent_when_settings_json_has_no_hooks_key() {
        let home = tmp("hooks-nokey-home");
        let cwd = tmp("hooks-nokey-cwd");
        let config = tmp("hooks-nokey-config");
        let claude_dir = cwd.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{ "model": "claude-sonnet-4-6" }"#,
        )
        .unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert!(
            findings.is_empty(),
            "expected no hooks when key absent: {findings:?}"
        );
    }

    #[test]
    fn hooks_detected_in_project_settings_json() {
        let home = tmp("hooks-proj-home");
        let cwd = tmp("hooks-proj-cwd");
        let config = tmp("hooks-proj-config");
        let claude_dir = cwd.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{ "hooks": { "PreToolUse": [], "PostToolUse": [] } }"#,
        )
        .unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert_eq!(findings.len(), 1, "expected 1 hooks finding");
        assert_eq!(findings[0].scope, "project");
        assert_eq!(findings[0].events, vec!["PostToolUse", "PreToolUse"]);
    }

    #[test]
    fn hooks_detected_in_user_level_settings() {
        let home = tmp("hooks-user-home");
        let cwd = tmp("hooks-user-cwd");
        let config = tmp("hooks-user-config");
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{ "hooks": { "Stop": [] } }"#,
        )
        .unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope, "user");
        assert_eq!(findings[0].events, vec!["Stop"]);
    }

    #[test]
    fn hooks_detected_in_settings_local_json() {
        let home = tmp("hooks-local-home");
        let cwd = tmp("hooks-local-cwd");
        let config = tmp("hooks-local-config");
        let claude_dir = cwd.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.local.json"),
            r#"{ "hooks": { "PreToolUse": [] } }"#,
        )
        .unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope, "project");
        assert_eq!(findings[0].events, vec!["PreToolUse"]);
    }

    #[test]
    fn hooks_tolerates_malformed_json_silently() {
        let home = tmp("hooks-malformed-home");
        let cwd = tmp("hooks-malformed-cwd");
        let config = tmp("hooks-malformed-config");
        let claude_dir = cwd.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("settings.json"), "{ not valid json }").unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        // Malformed JSON: no finding, no warning (parse silently skipped).
        assert!(
            findings.is_empty(),
            "expected no finding for malformed JSON"
        );
        assert!(
            warnings.is_empty(),
            "expected no warning for malformed JSON"
        );
    }

    #[test]
    fn hooks_detected_in_both_user_and_project() {
        let home = tmp("hooks-both-home");
        let cwd = tmp("hooks-both-cwd");
        let config = tmp("hooks-both-config");
        let home_claude = home.join(".claude");
        let cwd_claude = cwd.join(".claude");
        fs::create_dir_all(&home_claude).unwrap();
        fs::create_dir_all(&cwd_claude).unwrap();
        fs::write(
            home_claude.join("settings.json"),
            r#"{ "hooks": { "Stop": [] } }"#,
        )
        .unwrap();
        fs::write(
            cwd_claude.join("settings.json"),
            r#"{ "hooks": { "PreToolUse": [], "PostToolUse": [] } }"#,
        )
        .unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert_eq!(findings.len(), 2);
        let scopes: Vec<&str> = findings.iter().map(|f| f.scope).collect();
        assert!(scopes.contains(&"user"), "user scope missing: {scopes:?}");
        assert!(
            scopes.contains(&"project"),
            "project scope missing: {scopes:?}"
        );
    }

    #[test]
    fn hooks_empty_events_when_hooks_key_is_empty_object() {
        let home = tmp("hooks-empty-home");
        let cwd = tmp("hooks-empty-cwd");
        let config = tmp("hooks-empty-config");
        let claude_dir = cwd.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("settings.json"), r#"{ "hooks": {} }"#).unwrap();
        let roots = make_roots(home, cwd, config, ScanOs::MacOs);
        let mut warnings = Vec::new();
        let findings = collect_hooks(&roots, &mut warnings);
        assert_eq!(findings.len(), 1, "hooks key present even when empty");
        assert!(findings[0].events.is_empty(), "expected empty events list");
    }
}
