//! MCP config analyzer: the headline wedge. Client-agnostic by design. Paste a
//! Claude Desktop, Claude Code, Cursor, Continue, VS Code / Copilot, or Zed MCP
//! config and get the token cost of its tool definitions across cold, warm, and
//! Tool-Search scenarios, plus per-server recommendations to swap a server for
//! its official CLI where that saves tokens. See PLAN.md section 9.
//!
//! Tokenization is not done here. Tool-definition token costs come from a curated
//! catalog of representative cold-cache estimates (refreshable); for an unknown
//! server the surface asks the user to paste its `tools/list`.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::mcp_tools::ToolInventory;
use crate::pricing;
use crate::Provider;

/// 200K is the reference context window the percentage is measured against.
const WINDOW: f64 = 200_000.0;

/// Basis label for servers priced from the curated catalog.
const BASIS_CATALOG: &str = "catalog estimate (representative)";
/// Basis label for servers with no number at all (unknown, no manifest).
const BASIS_NONE: &str = "none (unknown server, no manifest supplied)";

/// What to do with a server: swap for a CLI, swap for ad hoc use only, or keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Recommendation {
    Replace,
    ReplaceForAdHoc,
    Keep,
    Unknown,
}

impl Recommendation {
    fn replaceable(self) -> bool {
        matches!(
            self,
            Recommendation::Replace | Recommendation::ReplaceForAdHoc
        )
    }
}

/// One verified way to register fewer tools for a server without removing it.
/// Serialized into the analysis output so surfaces can show the exact snippet.
#[derive(Clone, Debug, Serialize)]
pub struct SlimOption {
    /// e.g. "ENABLED_TOOLS env var".
    pub mechanism: String,
    /// The exact JSON fragment or args change to paste.
    pub snippet: String,
    /// Where the change goes: "env", "args", or "url".
    pub location: String,
    /// Estimated percentage of the cold tool-definition cost this removes.
    pub est_reduction_pct: u8,
    pub note: String,
    /// Short provenance, e.g. "verified against sooperset/mcp-atlassian docs, 2026-06".
    pub source_hint: String,
}

/// Per-server slim recommendation. `already_slimmed` means the config already
/// carries the slim env var / flag / url param, so the cold estimate was scaled
/// down and there is nothing left to save via this lever.
#[derive(Clone, Debug, Serialize)]
pub struct SlimRecommendation {
    pub already_slimmed: bool,
    pub option: SlimOption,
    pub est_tokens_saved: u64,
}

/// Static slim data for a catalog entry plus the markers used to detect a
/// config that is already slimmed. Detection checks key and flag PRESENCE
/// only; env values are never read or stored.
struct SlimSpec {
    mechanism: &'static str,
    snippet: &'static str,
    location: &'static str,
    est_reduction_pct: u8,
    note: &'static str,
    source_hint: &'static str,
    /// Env var names whose presence means the server is already slimmed.
    env_keys: &'static [&'static str],
    /// Args flags whose presence means the server is already slimmed.
    arg_flags: &'static [&'static str],
    /// Url substrings whose presence means the server is already slimmed.
    url_params: &'static [&'static str],
}

impl SlimSpec {
    fn to_option(&self) -> SlimOption {
        SlimOption {
            mechanism: self.mechanism.to_string(),
            snippet: self.snippet.to_string(),
            location: self.location.to_string(),
            est_reduction_pct: self.est_reduction_pct,
            note: self.note.to_string(),
            source_hint: self.source_hint.to_string(),
        }
    }

    fn detected(&self, s: &McpServer) -> bool {
        if self
            .env_keys
            .iter()
            .any(|k| s.env_keys.iter().any(|have| have == k))
        {
            return true;
        }
        if self.arg_flags.iter().any(|f| {
            s.args
                .iter()
                .any(|a| a == f || a.starts_with(&format!("{f}=")))
        }) {
            return true;
        }
        self.url_params
            .iter()
            .any(|p| s.url.as_deref().is_some_and(|u| u.contains(p)))
    }
}

struct CatalogEntry {
    id: &'static str,
    display: &'static str,
    /// Lowercase substrings matched against `name + command + args + url`.
    matchers: &'static [&'static str],
    tools: u32,
    /// Representative cold-cache token cost of this server's tool definitions.
    cold_tokens: u32,
    cli_alternative: Option<&'static str>,
    recommendation: Recommendation,
    note: &'static str,
    /// A verified register-fewer-tools mechanism, when the server has one.
    slim: Option<SlimSpec>,
    /// Verified to have NO native tool filtering. Triggers the client-side
    /// tool-search note at the analysis level.
    no_filtering: bool,
}

// Shared sentence for servers verified to have no native tool filtering.
const NO_FILTER_NOTE: &str = " No native tool filtering exists for this server; slim it client-side with MCP tool search or defer_loading.";

// Curated catalog (mid-2026, refreshable). Ordered so specific matchers win:
// github/gitlab are listed before the generic git server. Slim mechanisms and
// the no-filtering flags were verified against each server's docs in 2026-06.
static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "github",
        display: "GitHub",
        matchers: &["server-github", "github-mcp", "mcp-server-github"],
        tools: 90,
        cold_tokens: 40_000,
        cli_alternative: Some("gh"),
        recommendation: Recommendation::Replace,
        note: "The agent already knows gh. Replacing reclaims roughly 26K to 55K tokens (externally reported range; varies by server version and enabled toolsets); see the Tool Search column for the catalog's own defer-loaded figure.",
        slim: Some(SlimSpec {
            mechanism: "GITHUB_TOOLSETS env var",
            snippet: r#""env": { "GITHUB_TOOLSETS": "repos,issues" }"#,
            location: "env",
            est_reduction_pct: 50,
            note: "Comma list of toolsets (context, repos, issues, pull_requests, actions, code_security, and more). --tools selects individual tools; GITHUB_READ_ONLY=1 drops writes. The dynamic toolsets flag was removed; do not use it.",
            source_hint: "verified against github/github-mcp-server docs, 2026-06",
            env_keys: &["GITHUB_TOOLSETS"],
            arg_flags: &["--toolsets", "--tools"],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "gitlab",
        display: "GitLab",
        matchers: &["server-gitlab", "gitlab-mcp", "mcp-server-gitlab"],
        tools: 65,
        cold_tokens: 25_000,
        cli_alternative: Some("glab"),
        recommendation: Recommendation::Replace,
        note: "glab covers most flows. Keep the MCP only for features glab lacks.",
        slim: Some(SlimSpec {
            mechanism: "GITLAB_READ_ONLY_MODE env var",
            snippet: r#""env": { "GITLAB_READ_ONLY_MODE": "true" }"#,
            location: "env",
            est_reduction_pct: 50,
            note: "Community zereight/gitlab-mcp variant only; the official GitLab endpoint has no filtering. USE_GITLAB_WIKI, USE_MILESTONE, and USE_PIPELINE set to false drop those tool groups too.",
            source_hint: "verified against zereight/gitlab-mcp docs, 2026-06",
            env_keys: &["GITLAB_READ_ONLY_MODE"],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "git",
        display: "Git (local)",
        matchers: &["mcp-server-git", "mcp_server_git"],
        tools: 13,
        cold_tokens: 2_500,
        cli_alternative: Some("git"),
        recommendation: Recommendation::Replace,
        note: "The git CLI is universally known. The MCP wrapper adds tool-definition overhead with no new capability.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "filesystem",
        display: "Filesystem",
        matchers: &["server-filesystem", "filesystem-mcp"],
        tools: 11,
        cold_tokens: 2_000,
        cli_alternative: Some("bash + find/cat/rg"),
        recommendation: Recommendation::Replace,
        note: "Shell builtins cover file operations; a dedicated filesystem server is rarely needed.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "postgres",
        display: "Postgres",
        matchers: &["server-postgres", "postgres-mcp", "mcp-server-postgres"],
        tools: 20,
        cold_tokens: 6_000,
        cli_alternative: Some("psql"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "Use psql for ad hoc queries; keep the MCP for managed OAuth or pooling. --access-mode=restricted gates behavior, not tool count.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "neon",
        display: "Neon",
        matchers: &["neon"],
        tools: 20,
        cold_tokens: 6_000,
        cli_alternative: Some("neonctl"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "neonctl handles branches and SQL; keep the MCP for OAuth-gated flows.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "supabase",
        display: "Supabase",
        matchers: &["supabase"],
        tools: 22,
        cold_tokens: 5_000,
        cli_alternative: Some("supabase"),
        recommendation: Recommendation::Replace,
        note: "The supabase CLI mirrors the MCP toolset at a fraction of the token cost.",
        slim: Some(SlimSpec {
            mechanism: "features url parameter",
            snippet: "?features=database,docs&read_only=true&project_ref=<your-project-ref>",
            location: "url",
            est_reduction_pct: 70,
            note: "Feature groups: account, docs, database, debugging, development, functions, storage, branching. This is a change to the hosted endpoint url, not an env var.",
            source_hint: "verified against supabase mcp docs, 2026-06",
            env_keys: &[],
            arg_flags: &[],
            url_params: &["features="],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "notion",
        display: "Notion",
        matchers: &["notion"],
        tools: 21,
        cold_tokens: 26_000,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "No mature CLI. Keep the MCP, or try a slimmed fork (notion-slim, about 52% fewer tokens).",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "linear",
        display: "Linear",
        matchers: &["linear"],
        tools: 42,
        cold_tokens: 13_000,
        cli_alternative: Some("linear"),
        recommendation: Recommendation::Replace,
        note: "A Scalekit benchmark measured the linear CLI roughly 65x cheaper per task than the MCP.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "slack",
        display: "Slack",
        matchers: &["server-slack", "slack-mcp", "slack"],
        tools: 30,
        cold_tokens: 21_000,
        cli_alternative: Some("slack-cli"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "Use the CLI for posting and lookups; keep the MCP only for realtime / Socket Mode.",
        slim: Some(SlimSpec {
            mechanism: "SLACK_MCP_ENABLED_TOOLS env var",
            snippet: r#""env": { "SLACK_MCP_ENABLED_TOOLS": "conversations_history,channels_list" }"#,
            location: "env",
            est_reduction_pct: 75,
            note: "Comma list of the 13 tool names (korotovsky slack-mcp-server variant). Two or three tools cover most read flows.",
            source_hint: "verified against korotovsky/slack-mcp-server docs, 2026-06",
            env_keys: &["SLACK_MCP_ENABLED_TOOLS"],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "jira",
        display: "Jira / Atlassian",
        matchers: &["atlassian", "jira"],
        tools: 25,
        cold_tokens: 17_000,
        cli_alternative: Some("jira-cli"),
        recommendation: Recommendation::Replace,
        note: "jira-cli covers issue and sprint flows; replace unless you need Atlassian OAuth governance. jira-cli (ankitpokhrel) also supports Jira Server / Data Center on-prem with a custom URL: run jira init with installation type Local, set JIRA_API_TOKEN to a personal access token, and set JIRA_AUTH_TYPE=bearer. Atlassian's official acli is cloud-only. No mature on-prem Confluence read CLI exists (mark is publish-only), so for Confluence on-prem the slim env route is the primary recommendation.",
        slim: Some(SlimSpec {
            mechanism: "ENABLED_TOOLS env var",
            snippet: r#""env": { "ENABLED_TOOLS": "jira_search,jira_get_issue,jira_get_transitions,confluence_search" }"#,
            location: "env",
            est_reduction_pct: 90,
            note: "ENABLED_TOOLS takes a comma list of tool names; TOOLSETS=default trims the 68 tools to about 23 and TOOLSETS=all restores everything; READ_ONLY_MODE=true drops writes. When ENABLED_TOOLS and TOOLSETS are both set they intersect.",
            source_hint: "verified against sooperset/mcp-atlassian docs, 2026-06",
            env_keys: &["ENABLED_TOOLS", "TOOLSETS"],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "aws",
        display: "AWS",
        matchers: &["aws-mcp", "server-aws", "awslabs", "@aws"],
        tools: 30,
        cold_tokens: 17_000,
        cli_alternative: Some("aws"),
        recommendation: Recommendation::Replace,
        note: "Always prefer the aws CLI; the agent knows it well and the MCP tool surface is large. The slim pattern for AWS is installing fewer of its 60+ focused servers; the aws-api catch-all is already 2 to 3 tools, and READ_OPERATIONS_ONLY=true gates behavior, not tool count.",
        slim: None,
        no_filtering: false,
    },
    CatalogEntry {
        id: "kubernetes",
        display: "Kubernetes",
        matchers: &["kubernetes", "mcp-server-kubernetes", "k8s"],
        tools: 25,
        cold_tokens: 10_000,
        cli_alternative: Some("kubectl"),
        recommendation: Recommendation::Replace,
        note: "kubectl is the canonical interface; the MCP adds large tool definitions for no new capability.",
        slim: Some(SlimSpec {
            mechanism: "--toolsets args flag",
            snippet: r#""args": ["...", "--toolsets", "core", "--read-only"]"#,
            location: "args",
            est_reduction_pct: 40,
            note: "Comma list of toolsets (config and core are the defaults; helm, kcp, kiali, kubevirt, tekton are extras). --read-only and --disable-destructive drop write tools. Applies to containers/kubernetes-mcp-server.",
            source_hint: "verified against containers/kubernetes-mcp-server docs, 2026-06",
            env_keys: &[],
            arg_flags: &["--toolsets"],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "xcode",
        display: "XcodeBuildMCP",
        matchers: &["xcodebuild", "xcode"],
        tools: 60,
        cold_tokens: 12_600,
        cli_alternative: Some("xcodebuild / xcrun simctl"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "Use xcodebuild and xcrun simctl for builds and sims; keep the MCP for live log streaming.",
        slim: None,
        no_filtering: false,
    },
    CatalogEntry {
        id: "figma",
        display: "Figma",
        matchers: &["figma"],
        tools: 15,
        cold_tokens: 6_000,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "No mature CLI for the Figma read API. Keep the MCP.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "sentry",
        display: "Sentry",
        matchers: &["sentry"],
        tools: 16,
        cold_tokens: 9_000,
        cli_alternative: Some("sentry-cli"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "sentry-cli covers releases and issues; keep the MCP for interactive triage.",
        slim: Some(SlimSpec {
            mechanism: "MCP_DISABLE_SKILLS env var",
            snippet: r#""env": { "MCP_DISABLE_SKILLS": "seer" }"#,
            location: "env",
            est_reduction_pct: 25,
            note: "Comma list of skills to disable; granularity is skill level, not per tool.",
            source_hint: "verified against getsentry/sentry-mcp docs, 2026-06",
            env_keys: &["MCP_DISABLE_SKILLS"],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "brave-search",
        display: "Brave Search",
        matchers: &["brave-search", "brave"],
        tools: 2,
        cold_tokens: 1_500,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "Web search has no CLI equivalent and the token cost is already small. Keep.",
        slim: Some(SlimSpec {
            mechanism: "BRAVE_MCP_ENABLED_TOOLS env var",
            snippet: r#""env": { "BRAVE_MCP_ENABLED_TOOLS": "brave_web_search" }"#,
            location: "env",
            est_reduction_pct: 85,
            note: "Space separated list, not comma separated. The full server registers 8 tools; one tool covers most search use.",
            source_hint: "verified against brave/brave-search-mcp-server docs, 2026-06",
            env_keys: &["BRAVE_MCP_ENABLED_TOOLS"],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "playwright",
        display: "Playwright",
        matchers: &["playwright"],
        tools: 25,
        cold_tokens: 12_000,
        cli_alternative: Some("playwright"),
        recommendation: Recommendation::ReplaceForAdHoc,
        note: "The playwright CLI runs tests directly; keep the MCP for agent-driven interactive browsing.",
        slim: Some(SlimSpec {
            mechanism: "--caps args flag (opt-in groups)",
            snippet: "default is already slim; do not add --caps groups (vision, pdf, devtools, config, network, storage, testing) you do not use",
            location: "args",
            est_reduction_pct: 0,
            note: "The default 24 tools are already the slim profile; --caps opt-ins raise the count to 69.",
            source_hint: "verified against microsoft/playwright-mcp docs, 2026-06",
            env_keys: &[],
            arg_flags: &[],
            url_params: &[],
        }),
        no_filtering: false,
    },
    CatalogEntry {
        id: "puppeteer",
        display: "Puppeteer",
        matchers: &["puppeteer"],
        tools: 7,
        cold_tokens: 4_000,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "Browser automation has no simple CLI swap. Keep, or switch to a lighter automation server.",
        slim: None,
        no_filtering: false,
    },
    CatalogEntry {
        id: "fetch",
        display: "Fetch",
        matchers: &["server-fetch"],
        tools: 1,
        cold_tokens: 800,
        cli_alternative: Some("curl"),
        recommendation: Recommendation::Replace,
        note: "curl (or the agent's built-in fetch) covers HTTP; a dedicated fetch server is rarely worth its definition.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "memory",
        display: "Memory",
        matchers: &["server-memory"],
        tools: 9,
        cold_tokens: 3_000,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "No CLI equivalent; the memory server is a capability, not a wrapper. Keep.",
        slim: None,
        no_filtering: true,
    },
    CatalogEntry {
        id: "gdrive",
        display: "Google Drive",
        matchers: &["gdrive", "google-drive", "googledrive"],
        tools: 10,
        cold_tokens: 7_000,
        cli_alternative: None,
        recommendation: Recommendation::Keep,
        note: "No mature CLI for the Drive API. Keep the MCP.",
        slim: None,
        no_filtering: true,
    },
];

fn match_catalog(haystack: &str) -> Option<&'static CatalogEntry> {
    CATALOG
        .iter()
        .find(|e| e.matchers.iter().any(|m| haystack.contains(m)))
}

/// Cold/warm/Tool-Search/capacity token costs for one server.
#[derive(Clone, Debug, Serialize)]
pub struct Scenarios {
    pub cold: u64,
    pub warm: u64,
    pub tool_search: u64,
    pub capacity: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerReport {
    pub name: String,
    pub matched_id: Option<String>,
    pub display: String,
    pub transport: String,
    pub tools: Option<u32>,
    pub cold_tokens: Option<u32>,
    pub scenarios: Option<Scenarios>,
    pub cli_alternative: Option<String>,
    pub recommendation: Recommendation,
    pub note: String,
    pub savings_tokens: u64,
    pub savings_usd: f64,
    /// Register-fewer-tools recommendation, when the server has a verified
    /// slim mechanism. Independent of (and never added into) the CLI-swap
    /// savings: when a server has both, the swap is primary and slim is the
    /// "if you keep it, slim it" fallback.
    pub slim: Option<SlimRecommendation>,
    /// What kind of number `cold_tokens` is: "catalog estimate
    /// (representative)" or "tokenized manifest (<tokenizer>)". The two bases
    /// are never blended in one figure.
    pub basis: String,
    /// Exact per-tool breakdown, present only when a tools/list manifest was
    /// supplied for this server. Absent keys keep the no-manifest JSON shape
    /// byte-identical to previous releases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools_detail: Option<ToolInventory>,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpTotals {
    pub servers: usize,
    pub matched: usize,
    pub unknown: usize,
    /// Servers whose counts come from a supplied tools/list manifest (exact),
    /// as opposed to the catalog estimate. Unknown-but-measured servers count
    /// here and in `unknown`, and their exact numbers ARE in the totals.
    pub measured: usize,
    pub cold: u64,
    pub warm: u64,
    pub tool_search: u64,
    pub capacity: u64,
    pub pct_of_window: f64,
    pub savings_tokens: u64,
    pub savings_usd: f64,
    pub cold_usd: f64,
    /// Estimated savings from slimming servers you keep (registering fewer
    /// tools). Reported separately from the CLI-swap savings; the two are
    /// alternative levers per server and must not be summed.
    pub slim_savings_tokens: u64,
    pub slim_savings_usd: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpAnalysis {
    pub client: String,
    pub provider: Provider,
    pub servers: Vec<ServerReport>,
    pub totals: McpTotals,
    pub notes: Vec<String>,
}

struct McpServer {
    name: String,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    transport: String,
    /// Names of env vars present on the server entry. Keys only; values are
    /// never read or stored (slim detection needs presence, nothing more).
    env_keys: Vec<String>,
}

/// Analyze an MCP config (JSON or JSONC) for `provider`. Detects the client
/// shape automatically. Every server is priced from the curated catalog;
/// use [`analyze_with_inventories`] to supersede catalog estimates with
/// exact tokenized manifests.
pub fn analyze(config_text: &str, provider: Provider) -> Result<McpAnalysis, String> {
    analyze_with_inventories(config_text, provider, &BTreeMap::new())
}

/// Names of the servers in an MCP config, in the analyzer's iteration order
/// (serde_json's map sorts keys, so this is name order, deterministic). Lets
/// a caller validate a tools/list attachment target before tokenizing.
pub fn server_names(config_text: &str) -> Result<Vec<String>, String> {
    let cleaned = strip_jsonc(config_text);
    let value: Value = serde_json::from_str(&cleaned)
        .map_err(|e| format!("could not parse config as JSON/JSONC: {e}"))?;
    let (_, servers_val) = locate_servers(&value).ok_or_else(|| {
        "no MCP servers found (expected a mcpServers, servers, or context_servers object)"
            .to_string()
    })?;
    Ok(parse_servers(servers_val)
        .into_iter()
        .map(|s| s.name)
        .collect())
}

/// Analyze an MCP config with exact tool inventories attached to some servers
/// (keyed by the server name exactly as it appears in the config). Servers
/// with an inventory report exact tokenized-manifest counts that supersede
/// the catalog estimate; everything else keeps the catalog path. Errors when
/// an inventory key matches no server in the config.
pub fn analyze_with_inventories(
    config_text: &str,
    provider: Provider,
    inventories: &BTreeMap<String, ToolInventory>,
) -> Result<McpAnalysis, String> {
    let cleaned = strip_jsonc(config_text);
    let value: Value = serde_json::from_str(&cleaned)
        .map_err(|e| format!("could not parse config as JSON/JSONC: {e}"))?;

    let (client, servers_val) = locate_servers(&value).ok_or_else(|| {
        "no MCP servers found (expected a mcpServers, servers, or context_servers object)"
            .to_string()
    })?;

    let servers = parse_servers(servers_val);
    if servers.is_empty() {
        return Err("the config has a server section but no servers in it".to_string());
    }
    for key in inventories.keys() {
        if !servers.iter().any(|s| &s.name == key) {
            let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
            return Err(format!(
                "tools/list inventory targets server \"{key}\" but the config has: {}",
                names.join(", ")
            ));
        }
    }

    let reports: Vec<ServerReport> = servers
        .iter()
        .map(|s| {
            let haystack = format!(
                "{} {} {} {}",
                s.name,
                s.command.clone().unwrap_or_default(),
                s.args.join(" "),
                s.url.clone().unwrap_or_default()
            )
            .to_lowercase();
            build_report(
                s,
                match_catalog(&haystack),
                inventories.get(&s.name),
                provider,
            )
        })
        .collect();

    Ok(assemble(client.to_string(), provider, reports))
}

/// Wrap a single tokenized tool inventory in the standard analysis shape:
/// one server named `server_name`, matched against the catalog by name so
/// swap and slim recommendations still attach when the name is recognizable.
/// This is the no-config path ("here is a tools/list manifest, analyze it").
pub fn analysis_from_inventory(
    server_name: &str,
    provider: Provider,
    inventory: ToolInventory,
) -> McpAnalysis {
    let server = McpServer {
        name: server_name.to_string(),
        command: None,
        args: Vec::new(),
        url: None,
        transport: "manifest".to_string(),
        env_keys: Vec::new(),
    };
    let haystack = server_name.to_lowercase();
    let report = build_report(
        &server,
        match_catalog(&haystack),
        Some(&inventory),
        provider,
    );
    assemble(
        "tools/list manifest (single server)".to_string(),
        provider,
        vec![report],
    )
}

/// Build one server's report from its catalog entry (if matched) and exact
/// inventory (if supplied). The four combinations share this single path so
/// the basis labeling can never drift between them.
fn build_report(
    s: &McpServer,
    entry: Option<&'static CatalogEntry>,
    inventory: Option<&ToolInventory>,
    provider: Provider,
) -> ServerReport {
    let rate = pricing::default_for(provider).input;
    let per_million = |t: u64| (t as f64) / 1_000_000.0 * rate;

    match (entry, inventory) {
        (Some(entry), None) => {
            // Catalog estimate path, unchanged semantics.
            let already_slimmed = entry.slim.as_ref().is_some_and(|spec| spec.detected(s));
            let cold_tokens = if already_slimmed {
                let pct = entry.slim.as_ref().map_or(0, |spec| spec.est_reduction_pct);
                (u64::from(entry.cold_tokens) * u64::from(100 - pct.min(100)) / 100) as u32
            } else {
                entry.cold_tokens
            };

            let sc = scenarios(provider, entry.tools, cold_tokens);
            let savings = if entry.recommendation.replaceable() && entry.cli_alternative.is_some()
            {
                u64::from(cold_tokens)
            } else {
                0
            };
            let slim = entry.slim.as_ref().map(|spec| {
                let est_tokens_saved = if already_slimmed {
                    0
                } else {
                    u64::from(cold_tokens) * u64::from(spec.est_reduction_pct) / 100
                };
                SlimRecommendation {
                    already_slimmed,
                    option: spec.to_option(),
                    est_tokens_saved,
                }
            });

            let mut note = if entry.no_filtering {
                format!("{}{NO_FILTER_NOTE}", entry.note)
            } else {
                entry.note.to_string()
            };
            if already_slimmed {
                let spec = entry.slim.as_ref().expect("already_slimmed implies slim");
                note.push_str(&format!(
                    " Cold estimate adjusted down {}% because {} is already set in this config.",
                    spec.est_reduction_pct, spec.mechanism
                ));
            }

            ServerReport {
                name: s.name.clone(),
                matched_id: Some(entry.id.to_string()),
                display: entry.display.to_string(),
                transport: s.transport.clone(),
                tools: Some(entry.tools),
                cold_tokens: Some(cold_tokens),
                scenarios: Some(sc),
                cli_alternative: entry.cli_alternative.map(String::from),
                recommendation: entry.recommendation,
                note,
                savings_tokens: savings,
                savings_usd: per_million(savings),
                slim,
                basis: BASIS_CATALOG.to_string(),
                tools_detail: None,
            }
        }
        (entry, Some(inv)) => {
            // Exact tokenized-manifest path: the manifest is the measured
            // truth, so no already-slimmed downscaling applies (whatever
            // filtering the config carries is already reflected in the
            // manifest the server emitted).
            let cold_tokens = u32::try_from(inv.total_tokens).unwrap_or(u32::MAX);
            let sc = scenarios(provider, inv.tool_count, cold_tokens);
            let (recommendation, cli_alternative, display, matched_id) = match entry {
                Some(e) => (
                    e.recommendation,
                    e.cli_alternative.map(String::from),
                    e.display.to_string(),
                    Some(e.id.to_string()),
                ),
                None => (Recommendation::Unknown, None, s.name.clone(), None),
            };
            let savings = if recommendation.replaceable() && cli_alternative.is_some() {
                u64::from(cold_tokens)
            } else {
                0
            };
            let slim = entry.and_then(|e| e.slim.as_ref()).map(|spec| {
                let already_slimmed = spec.detected(s);
                let est_tokens_saved = if already_slimmed {
                    0
                } else {
                    u64::from(cold_tokens) * u64::from(spec.est_reduction_pct) / 100
                };
                SlimRecommendation {
                    already_slimmed,
                    option: spec.to_option(),
                    est_tokens_saved,
                }
            });

            let mut note = match entry {
                Some(e) if e.no_filtering => format!("{}{NO_FILTER_NOTE}", e.note),
                Some(e) => e.note.to_string(),
                None => "Not in the catalog.".to_string(),
            };
            note.push_str(
                " Exact counts from the supplied tools/list manifest supersede the catalog estimate for this server.",
            );
            if slim.as_ref().is_some_and(|x| x.est_tokens_saved > 0) {
                note.push_str(
                    " The slim figure applies the catalog's estimated reduction percentage to the measured total; re-run with a manifest captured from the slimmed server for an exact slim count.",
                );
            }

            ServerReport {
                name: s.name.clone(),
                matched_id,
                display,
                transport: s.transport.clone(),
                tools: Some(inv.tool_count),
                cold_tokens: Some(cold_tokens),
                scenarios: Some(sc),
                cli_alternative,
                recommendation,
                note,
                savings_tokens: savings,
                savings_usd: per_million(savings),
                slim,
                basis: inv.basis.clone(),
                tools_detail: Some(inv.clone()),
            }
        }
        (None, None) => ServerReport {
            name: s.name.clone(),
            matched_id: None,
            display: s.name.clone(),
            transport: s.transport.clone(),
            tools: None,
            cold_tokens: None,
            scenarios: None,
            cli_alternative: None,
            recommendation: Recommendation::Unknown,
            note: "Not in the catalog. Save this server's tools/list output as JSON and re-run with --tools-list <path> (or pass the manifest file directly) for exact tokenized counts."
                .to_string(),
            savings_tokens: 0,
            savings_usd: 0.0,
            slim: None,
            basis: BASIS_NONE.to_string(),
            tools_detail: None,
        },
    }
}

/// Fold per-server reports into totals and analysis-level notes.
fn assemble(client: String, provider: Provider, reports: Vec<ServerReport>) -> McpAnalysis {
    let rate = pricing::default_for(provider).input;
    let per_million = |t: u64| (t as f64) / 1_000_000.0 * rate;

    let (mut t_cold, mut t_warm, mut t_search, mut t_cap, mut t_save) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut t_slim = 0u64;
    let (mut matched, mut unknown, mut measured) = (0usize, 0usize, 0usize);
    let mut uncounted_unknown = 0usize;
    let mut any_no_filtering = false;
    let mut any_catalog_priced = false;

    for r in &reports {
        if r.matched_id.is_some() {
            matched += 1;
        } else {
            unknown += 1;
            if r.tools_detail.is_none() {
                uncounted_unknown += 1;
            }
        }
        if r.tools_detail.is_some() {
            measured += 1;
        } else if r.matched_id.is_some() {
            any_catalog_priced = true;
        }
        if r.note.contains("No native tool filtering") {
            any_no_filtering = true;
        }
        if let Some(sc) = &r.scenarios {
            t_cold += sc.cold;
            t_warm += sc.warm;
            t_search += sc.tool_search;
            t_cap += sc.capacity;
        }
        t_save += r.savings_tokens;
        t_slim += r.slim.as_ref().map_or(0, |x| x.est_tokens_saved);
    }

    let mut notes = Vec::new();
    if any_catalog_priced {
        notes.push(
            "Cold-cache token costs for catalog-matched servers are representative estimates. Supply a server's tools/list (CLI: --tools-list <path>, or pass the manifest file directly) for exact tokenized counts.".to_string(),
        );
    }
    if measured > 0 {
        notes.push(
            "Servers labeled \"tokenized manifest\" carry exact counts of the supplied tools/list (each tool serialized compactly as {name, description, input_schema}) and supersede catalog estimates. Estimates and exact counts are never blended into one figure.".to_string(),
        );
    }
    notes.push(
        "Tool Search (defer-loading) collapses tool definitions to a roughly 500-token stub plus a few tools loaded on demand.".to_string(),
    );
    notes.push(
        "Savings are input-token-bounded: swapping an MCP server for its CLI frees the tool-definition budget, but the agent must already know the CLI.".to_string(),
    );
    if t_slim > 0 {
        notes.push(
            "Slim savings assume you keep the server and register fewer tools. For a server that also has a CLI swap, the swap is the primary recommendation and slim is the fallback; never add the two figures for the same server.".to_string(),
        );
    }
    if any_no_filtering {
        notes.push(
            "Some servers here have no native tool filtering. The client-side lever is MCP tool search: Claude Code 2.1 ships it on by default (ENABLE_TOOL_SEARCH, with a per-server alwaysLoad opt-out), and the Claude API equivalent is the tool search tool with per-tool defer_loading: true. Cursor and VS Code expose per-tool toggles in their UIs.".to_string(),
        );
    }
    if uncounted_unknown > 0 {
        notes.push(format!(
            "{uncounted_unknown} server(s) were not recognized and are excluded from the totals."
        ));
    }

    let totals = McpTotals {
        servers: reports.len(),
        matched,
        unknown,
        measured,
        cold: t_cold,
        warm: t_warm,
        tool_search: t_search,
        capacity: t_cap,
        pct_of_window: (t_cold as f64 / WINDOW * 100.0 * 100.0).round() / 100.0,
        savings_tokens: t_save,
        savings_usd: per_million(t_save),
        cold_usd: per_million(t_cold),
        slim_savings_tokens: t_slim,
        slim_savings_usd: per_million(t_slim),
    };

    McpAnalysis {
        client,
        provider,
        servers: reports,
        totals,
        notes,
    }
}

pub(crate) fn scenarios(provider: Provider, tools: u32, cold: u32) -> Scenarios {
    // Cold and warm multipliers derive from the pricing table so this surface
    // stays in sync with the cost calculator. Cold is the first-turn cache
    // write surcharge (cache_write_5m / input, 1.0 when the provider does
    // not bill a write); warm is the subsequent-turn cached read discount
    // (cache_read / input, 1.0 when the provider publishes no discount).
    let (cold_mult, warm_mult) = cache_multipliers(provider);
    let cold_t = (f64::from(cold) * cold_mult).round() as u64;
    let warm_t = (f64::from(cold) * warm_mult).round() as u64;
    // Tool Search: a ~500-token search stub plus up to 5 tools loaded on demand
    // at ~600 tokens each.
    let tool_search = 500 + u64::from(tools.min(5)) * 600;
    Scenarios {
        cold: cold_t,
        warm: warm_t,
        tool_search,
        capacity: u64::from(cold),
    }
}

/// Cold (cache write) and warm (cache read) multipliers for `provider`,
/// derived from the provider's default model in the pricing table. Returns
/// 1.0 in either slot when the provider publishes no separate rate.
fn cache_multipliers(provider: Provider) -> (f64, f64) {
    let m = pricing::default_for(provider);
    let cold = m.cache_write_5m.map_or(1.0, |w| w / m.input);
    let warm = m.cache_read.map_or(1.0, |r| r / m.input);
    (cold, warm)
}

/// Find the servers map under any known client key and return a human label.
fn locate_servers(value: &Value) -> Option<(&'static str, &Value)> {
    if let Some(v) = value.get("mcpServers").filter(|v| v.is_object()) {
        return Some((
            "mcpServers (Claude Desktop / Claude Code / Cursor / Continue)",
            v,
        ));
    }
    if let Some(v) = value
        .get("mcp")
        .and_then(|m| m.get("servers"))
        .filter(|v| v.is_object())
    {
        return Some(("mcp.servers (VS Code settings.json)", v));
    }
    if let Some(v) = value.get("servers").filter(|v| v.is_object()) {
        return Some(("servers (VS Code / Copilot)", v));
    }
    if let Some(v) = value.get("context_servers").filter(|v| v.is_object()) {
        return Some(("context_servers (Zed)", v));
    }
    None
}

fn parse_servers(map: &Value) -> Vec<McpServer> {
    let Some(obj) = map.as_object() else {
        return Vec::new();
    };
    obj.iter().map(|(name, v)| parse_one(name, v)).collect()
}

fn parse_one(name: &str, obj: &Value) -> McpServer {
    // command may be a string (most clients) or an object { path, args } (Zed).
    let mut command = obj.get("command").and_then(Value::as_str).map(String::from);
    let mut args: Vec<String> = Vec::new();

    if let Some(cmd_obj) = obj.get("command").and_then(Value::as_object) {
        command = cmd_obj
            .get("path")
            .and_then(Value::as_str)
            .map(String::from);
        if let Some(a) = cmd_obj.get("args").and_then(Value::as_array) {
            args = a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
    }
    if args.is_empty() {
        if let Some(a) = obj.get("args").and_then(Value::as_array) {
            args = a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
    }

    let url = obj.get("url").and_then(Value::as_str).map(String::from);
    let transport = if url.is_some() {
        obj.get("type")
            .and_then(Value::as_str)
            .unwrap_or("sse")
            .to_string()
    } else {
        "stdio".to_string()
    };

    // Slim detection needs key presence only. Env VALUES are dropped right
    // here and never reach McpServer or any report.
    let env_keys = obj
        .get("env")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();

    McpServer {
        name: name.to_string(),
        command,
        args,
        url,
        transport,
        env_keys,
    }
}

/// Strip `//` and `/* */` comments and trailing commas so common JSONC configs
/// (VS Code, Cursor, Zed) parse with serde_json. String-aware, so delimiters
/// inside string values are preserved. Byte-level but UTF-8 safe: multibyte
/// sequences inside strings are copied verbatim. Crate-visible so the
/// tools/list parser (mcp_tools.rs) gets the same paste resilience.
pub(crate) fn strip_jsonc(s: &str) -> String {
    let b = s.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    let mut in_str = false;
    let mut esc = false;

    while i < n {
        let c = b[i];
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            i += 2;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }

    let no_comments = String::from_utf8(out).unwrap_or_default();
    remove_trailing_commas(&no_comments)
}

fn remove_trailing_commas(s: &str) -> String {
    let b = s.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    let mut in_str = false;
    let mut esc = false;

    while i < n {
        let c = b[i];
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == b',' {
            let mut j = i + 1;
            while j < n && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\n' || b[j] == b'\r') {
                j += 1;
            }
            if j < n && (b[j] == b'}' || b[j] == b']') {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_DESKTOP: &str = r#"{
      "mcpServers": {
        "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] },
        "files":  { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] }
      }
    }"#;

    #[test]
    fn parses_claude_desktop_and_matches() {
        let a = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        assert_eq!(a.servers.len(), 2);
        assert_eq!(a.totals.matched, 2);
        assert_eq!(a.totals.unknown, 0);
        let ids: Vec<_> = a
            .servers
            .iter()
            .filter_map(|s| s.matched_id.clone())
            .collect();
        assert!(ids.contains(&"github".to_string()));
        assert!(ids.contains(&"filesystem".to_string()));
        // both are replaceable, so savings is the sum of their cold tokens
        assert_eq!(a.totals.savings_tokens, 40_000 + 2_000);
        assert!(
            a.totals.cold > a.totals.capacity,
            "cold has the 1.25x anthropic surcharge"
        );
    }

    #[test]
    fn anthropic_cold_surcharge_and_warm_discount() {
        let a = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        let gh = a
            .servers
            .iter()
            .find(|s| s.matched_id.as_deref() == Some("github"))
            .unwrap();
        let sc = gh.scenarios.as_ref().unwrap();
        assert_eq!(sc.capacity, 40_000);
        // Derive the expected cold/warm from the pricing table rather than
        // hardcoding 1.25/0.10 so a future Anthropic rate change does not
        // silently break this assertion.
        let (cold_mult, warm_mult) = cache_multipliers(Provider::Anthropic);
        assert_eq!(sc.cold, (40_000.0 * cold_mult).round() as u64);
        assert_eq!(sc.warm, (40_000.0 * warm_mult).round() as u64);
        assert_eq!(sc.tool_search, 500 + 5 * 600); // capped at 5 tools
    }

    #[test]
    fn vscode_servers_shape() {
        let cfg = r#"{ "servers": { "gh": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#;
        let a = analyze(cfg, Provider::OpenAi).unwrap();
        assert!(a.client.contains("servers"));
        assert_eq!(a.totals.matched, 1);
        let sc = a.servers[0].scenarios.as_ref().unwrap();
        let (cold_mult, warm_mult) = cache_multipliers(Provider::OpenAi);
        assert_eq!(sc.cold, (40_000.0 * cold_mult).round() as u64);
        assert_eq!(sc.warm, (40_000.0 * warm_mult).round() as u64);
    }

    #[test]
    fn zed_context_servers_with_command_object() {
        let cfg = r#"{ "context_servers": { "linear": { "command": { "path": "npx", "args": ["linear-mcp"] } } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert!(a.client.contains("Zed"));
        assert_eq!(a.servers[0].matched_id.as_deref(), Some("linear"));
        assert_eq!(a.servers[0].cli_alternative.as_deref(), Some("linear"));
    }

    #[test]
    fn jsonc_comments_and_trailing_commas() {
        let cfg = r#"{
          // my servers
          "mcpServers": {
            "aws": { "command": "uvx", "args": ["awslabs.aws-mcp"], }, /* trailing comma + block comment */
          }
        }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert_eq!(a.servers[0].matched_id.as_deref(), Some("aws"));
    }

    #[test]
    fn unknown_server_is_flagged_not_counted() {
        let cfg = r#"{ "mcpServers": { "homegrown": { "command": "node", "args": ["./my-server.js"] } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert_eq!(a.totals.unknown, 1);
        assert_eq!(a.totals.matched, 0);
        assert_eq!(a.totals.cold, 0);
        assert_eq!(a.servers[0].recommendation, Recommendation::Unknown);
    }

    #[test]
    fn url_based_server_matches_by_name() {
        let cfg = r#"{ "mcpServers": { "linear": { "url": "https://mcp.linear.app/sse", "type": "sse" } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert_eq!(a.servers[0].matched_id.as_deref(), Some("linear"));
        assert_eq!(a.servers[0].transport, "sse");
    }

    #[test]
    fn keep_server_has_no_savings() {
        let cfg = r#"{ "mcpServers": { "notion": { "command": "npx", "args": ["@notionhq/notion-mcp-server"] } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert_eq!(a.servers[0].recommendation, Recommendation::Keep);
        assert_eq!(a.servers[0].savings_tokens, 0);
    }

    #[test]
    fn errors_when_no_server_section() {
        assert!(analyze(r#"{ "foo": 1 }"#, Provider::Anthropic).is_err());
    }

    #[test]
    fn result_serializes_to_json() {
        let a = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"totals\""));
        assert!(json.contains("\"recommendation\""));
        assert!(json.contains("replace"));
    }

    #[test]
    fn slim_recommendation_for_unslimmed_github() {
        let a = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        let gh = a
            .servers
            .iter()
            .find(|s| s.matched_id.as_deref() == Some("github"))
            .unwrap();
        let slim = gh.slim.as_ref().expect("github has a slim mechanism");
        assert!(!slim.already_slimmed);
        // 40_000 cold * 50% reduction
        assert_eq!(slim.est_tokens_saved, 20_000);
        assert_eq!(slim.option.location, "env");
        assert!(slim.option.snippet.contains("GITHUB_TOOLSETS"));
        assert!(slim.option.snippet.contains("repos,issues"));
        assert!(slim.option.source_hint.contains("github-mcp-server"));
    }

    #[test]
    fn already_slimmed_via_env_downscales_cold() {
        let cfg = r#"{ "mcpServers": { "github": {
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": { "GITHUB_TOOLSETS": "repos,issues", "GITHUB_TOKEN": "x" }
        } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let gh = &a.servers[0];
        let slim = gh.slim.as_ref().unwrap();
        assert!(slim.already_slimmed);
        assert_eq!(slim.est_tokens_saved, 0);
        // cold estimate scaled down by the 50% reduction before scenario math
        assert_eq!(gh.cold_tokens, Some(20_000));
        let sc = gh.scenarios.as_ref().unwrap();
        assert_eq!(sc.capacity, 20_000);
        assert_eq!(sc.cold, 25_000); // 20_000 * 1.25 anthropic surcharge
        assert!(gh.note.contains("Cold estimate adjusted down 50%"));
        // the CLI-swap savings also work from the adjusted estimate
        assert_eq!(gh.savings_tokens, 20_000);
        assert_eq!(a.totals.slim_savings_tokens, 0);
    }

    #[test]
    fn already_slimmed_via_args_flag() {
        let cfg = r#"{ "mcpServers": { "k8s": {
            "command": "kubernetes-mcp-server",
            "args": ["--toolsets", "core", "--read-only"]
        } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let k8s = &a.servers[0];
        assert_eq!(k8s.matched_id.as_deref(), Some("kubernetes"));
        let slim = k8s.slim.as_ref().unwrap();
        assert!(slim.already_slimmed);
        assert_eq!(slim.est_tokens_saved, 0);
        assert_eq!(k8s.cold_tokens, Some(6_000)); // 10_000 minus 40%
    }

    #[test]
    fn already_slimmed_via_url_param() {
        let cfg = r#"{ "mcpServers": { "supabase": {
            "url": "https://mcp.supabase.com/mcp?features=database,docs&read_only=true&project_ref=abc",
            "type": "http"
        } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let sb = &a.servers[0];
        assert_eq!(sb.matched_id.as_deref(), Some("supabase"));
        assert!(sb.slim.as_ref().unwrap().already_slimmed);
        assert_eq!(sb.cold_tokens, Some(1_500)); // 5_000 minus 70%
    }

    #[test]
    fn jira_slim_and_on_prem_note_correction() {
        let cfg = r#"{ "mcpServers": { "atlassian": {
            "command": "uvx", "args": ["mcp-atlassian"]
        } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let jira = &a.servers[0];
        assert_eq!(jira.matched_id.as_deref(), Some("jira"));
        // on-prem correction landed in the catalog note
        assert!(jira.note.contains("Jira Server / Data Center"));
        assert!(jira.note.contains("JIRA_AUTH_TYPE=bearer"));
        assert!(jira.note.contains("acli is cloud-only"));
        assert!(jira.note.contains("mark is publish-only"));
        let slim = jira.slim.as_ref().unwrap();
        assert!(!slim.already_slimmed);
        assert_eq!(slim.est_tokens_saved, 15_300); // 17_000 * 90%
        assert!(slim.option.snippet.contains("ENABLED_TOOLS"));
        assert!(slim.option.note.contains("they intersect"));

        // TOOLSETS alone also counts as already slimmed
        let cfg2 = r#"{ "mcpServers": { "atlassian": {
            "command": "uvx", "args": ["mcp-atlassian"],
            "env": { "TOOLSETS": "default" }
        } } }"#;
        let a2 = analyze(cfg2, Provider::Anthropic).unwrap();
        assert!(a2.servers[0].slim.as_ref().unwrap().already_slimmed);
    }

    #[test]
    fn totals_keep_slim_and_swap_savings_separate() {
        let cfg = r#"{ "mcpServers": {
            "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] },
            "brave": { "command": "npx", "args": ["@brave/brave-search-mcp-server"] }
        } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        // swap savings: github only (brave is a keep with no CLI)
        assert_eq!(a.totals.savings_tokens, 40_000);
        // slim savings: github 20_000 + brave 1_275 (1_500 * 85%), never
        // folded into savings_tokens
        assert_eq!(a.totals.slim_savings_tokens, 20_000 + 1_275);
        assert!(a
            .notes
            .iter()
            .any(|n| n.contains("never add the two figures")));
    }

    #[test]
    fn no_filtering_server_gets_note_and_client_side_lever() {
        let cfg = r#"{ "mcpServers": { "notion": { "command": "npx", "args": ["@notionhq/notion-mcp-server"] } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let notion = &a.servers[0];
        assert!(notion.slim.is_none());
        assert!(notion.note.contains("No native tool filtering"));
        assert!(a
            .notes
            .iter()
            .any(|n| n.contains("ENABLE_TOOL_SEARCH") && n.contains("defer_loading")));
    }

    #[test]
    fn slim_snippets_carry_verified_syntax() {
        let cfg = r#"{ "mcpServers": {
            "slack": { "command": "npx", "args": ["slack-mcp-server"] },
            "brave": { "command": "npx", "args": ["@brave/brave-search-mcp-server"] }
        } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let slack = a
            .servers
            .iter()
            .find(|s| s.matched_id.as_deref() == Some("slack"))
            .unwrap();
        let brave = a
            .servers
            .iter()
            .find(|s| s.matched_id.as_deref() == Some("brave-search"))
            .unwrap();
        assert!(slack
            .slim
            .as_ref()
            .unwrap()
            .option
            .snippet
            .contains("SLACK_MCP_ENABLED_TOOLS"));
        let brave_opt = &brave.slim.as_ref().unwrap().option;
        assert!(brave_opt.snippet.contains("BRAVE_MCP_ENABLED_TOOLS"));
        assert!(brave_opt.note.contains("Space separated"));
    }

    #[test]
    fn cache_multipliers_derive_from_pricing_table() {
        // Anthropic: cache_write_5m / input and cache_read / input from the
        // default model (Sonnet 4.6: 3.75 / 3.0 = 1.25, 0.30 / 3.0 = 0.10).
        let anth_default = pricing::default_for(Provider::Anthropic);
        let expected_anth_cold = anth_default.cache_write_5m.unwrap() / anth_default.input;
        let expected_anth_warm = anth_default.cache_read.unwrap() / anth_default.input;
        let (anth_cold, anth_warm) = cache_multipliers(Provider::Anthropic);
        assert!((anth_cold - expected_anth_cold).abs() < 1e-12);
        assert!((anth_warm - expected_anth_warm).abs() < 1e-12);
        assert!((anth_cold - 1.25).abs() < 1e-12);
        assert!((anth_warm - 0.10).abs() < 1e-12);

        // OpenAI: no cache_write rate published (defaults to 1.0); cache_read
        // is 10% of base across the GPT-5 family. Default model is gpt-5.4.
        let oai_default = pricing::default_for(Provider::OpenAi);
        assert!(oai_default.cache_write_5m.is_none());
        let expected_oai_warm = oai_default.cache_read.unwrap() / oai_default.input;
        let (oai_cold, oai_warm) = cache_multipliers(Provider::OpenAi);
        assert!((oai_cold - 1.0).abs() < 1e-12);
        assert!((oai_warm - expected_oai_warm).abs() < 1e-12);
        assert!((oai_warm - 0.10).abs() < 1e-12);

        // Gemini: cached tokens bill at 10% of base input on the 2.5 family;
        // no cache_write rate is modeled. Default model is gemini-2.5-pro.
        let gem_default = pricing::default_for(Provider::Gemini);
        assert!(gem_default.cache_write_5m.is_none());
        let expected_gem_warm = gem_default.cache_read.unwrap() / gem_default.input;
        let (gem_cold, gem_warm) = cache_multipliers(Provider::Gemini);
        assert!((gem_cold - 1.0).abs() < 1e-12);
        assert!((gem_warm - expected_gem_warm).abs() < 1e-12);
        assert!((gem_warm - 0.10).abs() < 1e-12);
    }

    #[test]
    fn warm_cold_scenarios_match_pricing_derived_values() {
        // Same 40_000-cold github fixture, each provider. Expectations are
        // derived from the pricing table at runtime, never hardcoded.
        for p in [Provider::Anthropic, Provider::OpenAi, Provider::Gemini] {
            let a = analyze(CLAUDE_DESKTOP, p).unwrap();
            let gh = a
                .servers
                .iter()
                .find(|s| s.matched_id.as_deref() == Some("github"))
                .unwrap();
            let sc = gh.scenarios.as_ref().unwrap();
            let (cold_mult, warm_mult) = cache_multipliers(p);
            assert_eq!(sc.capacity, 40_000, "{p:?}");
            assert_eq!(sc.cold, (40_000.0 * cold_mult).round() as u64, "{p:?}");
            assert_eq!(sc.warm, (40_000.0 * warm_mult).round() as u64, "{p:?}");
        }
    }

    #[test]
    fn github_note_does_not_contradict_tool_search_figure() {
        // P1-5 alignment: the catalog note must not carry a Tool-Search
        // figure that disagrees with the computed `tool_search` scenario.
        // Option (b) at landing time: keep the generic stub+tools formula
        // and rewrite the note to point readers at the scenario column.
        let a = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        let gh = a
            .servers
            .iter()
            .find(|s| s.matched_id.as_deref() == Some("github"))
            .unwrap();
        // No bare Tool-Search dollar/token figure in the note.
        assert!(
            !gh.note.contains("8.7K") && !gh.note.contains("8.7k"),
            "stale 8.7K Tool Search figure leaked into github note: {}",
            gh.note
        );
        // The retained external range is now explicitly labeled as external
        // (it is a multi-source community range, not any single benchmark).
        assert!(
            gh.note.contains("externally reported"),
            "external range label missing: {}",
            gh.note
        );
        // The computed Tool Search scenario stays the canonical figure.
        let sc = gh.scenarios.as_ref().unwrap();
        assert_eq!(sc.tool_search, 500 + 5 * 600);
    }

    fn sample_inventory(total: u64, count: u32) -> ToolInventory {
        use crate::mcp_tools::{analyze_tool_inventory, ToolTokenCount};
        // Spread the total over `count` synthetic tools; remainder on the
        // first tool so the sum is exact.
        let base = total / u64::from(count);
        let mut tools: Vec<ToolTokenCount> = (0..count)
            .map(|i| ToolTokenCount {
                name: format!("tool_{i}"),
                description: format!("Get item {i} from the store. Returns JSON."),
                tokens: base,
                description_tokens: 9,
            })
            .collect();
        tools[0].tokens += total - base * u64::from(count);
        analyze_tool_inventory(&tools, "o200k_base").unwrap()
    }

    #[test]
    fn inventory_supersedes_catalog_for_that_server() {
        let inv = sample_inventory(5_049, 14);
        let mut map = BTreeMap::new();
        map.insert("files".to_string(), inv);
        let a = analyze_with_inventories(CLAUDE_DESKTOP, Provider::Anthropic, &map).unwrap();

        let files = a.servers.iter().find(|s| s.name == "files").unwrap();
        // Catalog says filesystem cold is 2_000; the manifest says 5_049.
        assert_eq!(files.cold_tokens, Some(5_049));
        assert_eq!(files.tools, Some(14));
        assert_eq!(files.basis, "tokenized manifest (o200k_base)");
        assert!(files.tools_detail.is_some());
        assert!(files.note.contains("supersede the catalog estimate"));
        // Replaceable, so swap savings equal the exact cold.
        assert_eq!(files.savings_tokens, 5_049);

        // The github server keeps the catalog path and label.
        let gh = a.servers.iter().find(|s| s.name == "github").unwrap();
        assert_eq!(gh.basis, "catalog estimate (representative)");
        assert!(gh.tools_detail.is_none());

        // Totals mix correctly: capacity = 40_000 catalog + 5_049 exact.
        assert_eq!(a.totals.capacity, 45_049);
        assert_eq!(a.totals.measured, 1);
        assert_eq!(a.totals.matched, 2);
        // Both basis notes present for the mixed analysis.
        assert!(a
            .notes
            .iter()
            .any(|n| n.contains("representative estimates")));
        assert!(a.notes.iter().any(|n| n.contains("never blended")));
    }

    #[test]
    fn inventory_for_unknown_server_is_counted() {
        let cfg = r#"{ "mcpServers": { "homegrown": { "command": "node", "args": ["./my-server.js"] } } }"#;
        let inv = sample_inventory(1_200, 3);
        let mut map = BTreeMap::new();
        map.insert("homegrown".to_string(), inv);
        let a = analyze_with_inventories(cfg, Provider::Anthropic, &map).unwrap();
        let s = &a.servers[0];
        assert_eq!(s.recommendation, Recommendation::Unknown);
        assert_eq!(s.cold_tokens, Some(1_200));
        assert_eq!(a.totals.capacity, 1_200);
        assert_eq!(a.totals.unknown, 1);
        assert_eq!(a.totals.measured, 1);
        // Measured unknowns are not "excluded from the totals".
        assert!(!a
            .notes
            .iter()
            .any(|n| n.contains("excluded from the totals")));
    }

    #[test]
    fn inventory_key_must_match_a_server() {
        let inv = sample_inventory(100, 1);
        let mut map = BTreeMap::new();
        map.insert("nope".to_string(), inv);
        let err = analyze_with_inventories(CLAUDE_DESKTOP, Provider::Anthropic, &map).unwrap_err();
        assert!(err.contains("nope"));
        assert!(err.contains("github"));
    }

    #[test]
    fn inventory_skips_already_slimmed_downscale() {
        // GITHUB_TOOLSETS is set, but the manifest is the measured truth:
        // no downscaling, no adjustment note, slim est 0 (already slimmed).
        let cfg = r#"{ "mcpServers": { "github": {
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": { "GITHUB_TOOLSETS": "repos,issues" }
        } } }"#;
        let inv = sample_inventory(9_000, 27);
        let mut map = BTreeMap::new();
        map.insert("github".to_string(), inv);
        let a = analyze_with_inventories(cfg, Provider::Anthropic, &map).unwrap();
        let gh = &a.servers[0];
        assert_eq!(gh.cold_tokens, Some(9_000));
        assert!(!gh.note.contains("Cold estimate adjusted down"));
        let slim = gh.slim.as_ref().unwrap();
        assert!(slim.already_slimmed);
        assert_eq!(slim.est_tokens_saved, 0);
    }

    #[test]
    fn inventory_slim_estimate_is_labeled_mixed_basis() {
        let inv = sample_inventory(13_700, 43);
        let mut map = BTreeMap::new();
        map.insert("github".to_string(), inv);
        let a = analyze_with_inventories(CLAUDE_DESKTOP, Provider::Anthropic, &map).unwrap();
        let gh = a.servers.iter().find(|s| s.name == "github").unwrap();
        let slim = gh.slim.as_ref().unwrap();
        // 50% of the measured 13_700.
        assert_eq!(slim.est_tokens_saved, 6_850);
        assert!(gh
            .note
            .contains("applies the catalog's estimated reduction percentage"));
    }

    #[test]
    fn server_names_lists_deterministic_name_order() {
        let names = server_names(CLAUDE_DESKTOP).unwrap();
        assert_eq!(names, vec!["files".to_string(), "github".to_string()]);
        assert!(server_names(r#"{ "foo": 1 }"#).is_err());
    }

    #[test]
    fn analysis_from_inventory_matches_catalog_by_name() {
        let inv = sample_inventory(5_049, 14);
        let a = analysis_from_inventory("server-filesystem", Provider::Anthropic, inv);
        assert_eq!(a.client, "tools/list manifest (single server)");
        assert_eq!(a.totals.servers, 1);
        assert_eq!(a.totals.measured, 1);
        let s = &a.servers[0];
        assert_eq!(s.matched_id.as_deref(), Some("filesystem"));
        assert_eq!(s.recommendation, Recommendation::Replace);
        assert_eq!(s.cold_tokens, Some(5_049));
        assert_eq!(s.savings_tokens, 5_049);
        assert_eq!(s.basis, "tokenized manifest (o200k_base)");
        let sc = s.scenarios.as_ref().unwrap();
        assert_eq!(sc.capacity, 5_049);
        // Anthropic cold surcharge applies to the exact total.
        let (cold_mult, _) = cache_multipliers(Provider::Anthropic);
        assert_eq!(sc.cold, (5_049.0 * cold_mult).round() as u64);
    }

    #[test]
    fn analysis_from_inventory_unmatched_name_stays_unknown() {
        let inv = sample_inventory(900, 2);
        let a = analysis_from_inventory("homegrown-tools", Provider::Anthropic, inv);
        let s = &a.servers[0];
        assert_eq!(s.matched_id, None);
        assert_eq!(s.recommendation, Recommendation::Unknown);
        assert_eq!(s.cold_tokens, Some(900));
        assert_eq!(s.savings_tokens, 0);
        assert_eq!(a.totals.capacity, 900);
    }

    #[test]
    fn unknown_server_prompt_points_at_tools_list_flag() {
        let cfg = r#"{ "mcpServers": { "homegrown": { "command": "node", "args": ["./my-server.js"] } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        assert!(a.servers[0].note.contains("--tools-list"));
        assert_eq!(a.servers[0].basis, BASIS_NONE);
    }

    #[test]
    fn basis_and_measured_serialize() {
        let inv = sample_inventory(100, 1);
        let a = analysis_from_inventory("server-memory", Provider::Anthropic, inv);
        let json = serde_json::to_string(&a).unwrap();
        for key in [
            "\"basis\"",
            "\"tools_detail\"",
            "\"measured\"",
            "\"smells\"",
        ] {
            assert!(json.contains(key), "missing {key}");
        }
        // The no-manifest output omits tools_detail entirely (shape parity
        // with previous releases).
        let plain = analyze(CLAUDE_DESKTOP, Provider::Anthropic).unwrap();
        let plain_json = serde_json::to_string(&plain).unwrap();
        assert!(!plain_json.contains("tools_detail"));
        assert!(plain_json.contains("\"basis\""));
    }

    #[test]
    fn slim_fields_serialize_snake_case() {
        let cfg = r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#;
        let a = analyze(cfg, Provider::Anthropic).unwrap();
        let json = serde_json::to_string(&a).unwrap();
        for key in [
            "\"slim\"",
            "\"already_slimmed\"",
            "\"est_tokens_saved\"",
            "\"est_reduction_pct\"",
            "\"source_hint\"",
            "\"slim_savings_tokens\"",
            "\"slim_savings_usd\"",
        ] {
            assert!(json.contains(key), "missing {key}");
        }
    }
}
