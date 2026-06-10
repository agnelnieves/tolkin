//! Repo-wide agent-context discovery and analysis for `tolkin project`.
//! Strictly read-only: walks one directory tree (gitignore-aware), classifies
//! files into agent-context load profiles, token-counts and audits each, and
//! rolls everything up into a report. Nothing is written or transmitted.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use serde::Serialize;
use tolkin_core::audit::{self, AuditOptions};
use tolkin_core::mcp;
use tolkin_core::redact::{self, RedactOptions};
use tolkin_core::Provider as CoreProvider;

use crate::tokenize::{self, Provider as TokProvider};

/// When a file's content reaches the model relative to a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    /// In context every session before any work happens.
    Always,
    /// Loaded when a skill/command/agent is triggered.
    OnInvocation,
    /// Referenced lazily (skill resources, prompt libraries).
    OnDemand,
    /// Root markdown agents commonly read; reported with lower emphasis.
    Docs,
}

/// How a discovered path should be processed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain {
        class: &'static str,
        profile: Profile,
    },
    /// SKILL.md: frontmatter weighs in `always`, body in `on-invocation`.
    Skill,
    /// Analyzed via `mcp::analyze`; cold tokens fold into the `always` bucket.
    McpConfig,
}

/// Classify a path (relative to the scanned root) into an agent-context kind.
/// `None` means the file never reaches the model through agent plumbing.
pub fn classify(rel: &Path) -> Option<Kind> {
    let comps: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    let name = *comps.last()?;
    let in_dir = |a: &str, b: &str| comps.windows(2).any(|w| w[0] == a && w[1] == b);
    let is_md = name.ends_with(".md");

    // MCP configs first: they are analyzed structurally, not token-counted as
    // text, so they must win over any overlapping text class.
    if name == ".mcp.json"
        || ends_with_pair(&comps, ".cursor", "mcp.json")
        || ends_with_pair(&comps, ".vscode", "mcp.json")
        || ends_with_pair(&comps, ".zed", "settings.json")
        || ends_with_pair(&comps, ".claude", "settings.json")
    {
        return Some(Kind::McpConfig);
    }
    if name == "SKILL.md" {
        return Some(Kind::Skill);
    }
    let plain = |class, profile| Some(Kind::Plain { class, profile });
    match name {
        "CLAUDE.md" => return plain("claude-md", Profile::Always),
        "AGENTS.md" => return plain("agents-md", Profile::Always),
        "GEMINI.md" => return plain("gemini-md", Profile::Always),
        ".cursorrules" => return plain("cursorrules", Profile::Always),
        ".windsurfrules" => return plain("windsurfrules", Profile::Always),
        "copilot-instructions.md" if ends_with_pair(&comps, ".github", name) => {
            return plain("copilot-instructions", Profile::Always);
        }
        _ => {}
    }
    if in_dir(".cursor", "rules") {
        return plain("cursor-rule", Profile::Always);
    }
    if is_md && in_dir(".claude", "commands") {
        return plain("command", Profile::OnInvocation);
    }
    if is_md && in_dir(".claude", "agents") {
        return plain("agent", Profile::OnInvocation);
    }
    if is_md && in_dir(".codex", "prompts") {
        return plain("codex-prompt", Profile::OnInvocation);
    }
    // Non-SKILL.md files inside a skill directory are lazily referenced docs
    // and scripts.
    if in_dir(".claude", "skills") {
        return plain("skill-resource", Profile::OnDemand);
    }
    if is_md
        && comps[..comps.len() - 1]
            .iter()
            .any(|c| *c == "prompts" || *c == "prompt")
    {
        return plain("prompt-doc", Profile::OnDemand);
    }
    if is_md && comps.len() == 1 {
        return plain("doc", Profile::Docs);
    }
    None
}

fn ends_with_pair(comps: &[&str], a: &str, b: &str) -> bool {
    comps.len() >= 2 && comps[comps.len() - 2] == a && comps[comps.len() - 1] == b
}

/// A SKILL.md split at its frontmatter fences. The frontmatter (name and
/// description) lives in the agent's skill registry, so it costs tokens every
/// session; the body only loads when the skill triggers.
#[derive(Debug)]
pub struct SkillSplit {
    pub frontmatter: Option<String>,
    pub body: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Split on the leading `---` fence pair by plain line matching (no YAML
/// parser: only the fence positions and two known keys matter here).
pub fn split_skill(text: &str) -> SkillSplit {
    let lines: Vec<&str> = text.lines().collect();
    let opens = lines.first().map(|l| l.trim() == "---").unwrap_or(false);
    let close = opens
        .then(|| lines[1..].iter().position(|l| l.trim() == "---"))
        .flatten();
    match close {
        Some(close) => {
            let frontmatter = lines[1..1 + close].join("\n");
            let body = lines[(2 + close).min(lines.len())..].join("\n");
            let name = frontmatter_field(&frontmatter, "name");
            let description = frontmatter_field(&frontmatter, "description");
            SkillSplit {
                frontmatter: Some(frontmatter),
                body,
                name,
                description,
            }
        }
        None => SkillSplit {
            frontmatter: None,
            body: text.to_string(),
            name: None,
            description: None,
        },
    }
}

fn frontmatter_field(frontmatter: &str, key: &str) -> Option<String> {
    frontmatter.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.trim_start().strip_prefix(':')?;
        let value = rest
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        (!value.is_empty()).then_some(value)
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectOptions {
    pub experimental: bool,
    pub max_file_bytes: u64,
    pub top: usize,
}

/// One finding, slimmed for the per-file report (full spans and previews are
/// available via `tolkin audit <file>`).
#[derive(Clone, Debug, Serialize)]
pub struct FindingSummary {
    pub rule: String,
    pub severity: String,
    pub savings_min: u64,
    pub savings_max: u64,
}

/// One classified file (a SKILL.md with frontmatter yields two entries that
/// share a path: skill-frontmatter in `always`, skill-body in
/// `on-invocation`). Secret data is counts and kinds only, never values.
#[derive(Clone, Debug, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub class: String,
    pub profile: Profile,
    pub tokens: u64,
    pub findings: Vec<FindingSummary>,
    pub secret_count: usize,
    pub secret_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_description: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProfileRollup {
    pub tokens: u64,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Default, Serialize)]
pub struct Profiles {
    pub always: ProfileRollup,
    pub on_invocation: ProfileRollup,
    pub on_demand: ProfileRollup,
    pub docs: ProfileRollup,
}

#[derive(Debug, Serialize)]
pub struct McpConfigEntry {
    pub path: String,
    pub servers: usize,
    pub cold_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct HeavyFile {
    pub path: String,
    pub tokens: u64,
    pub findings: usize,
    pub savings_min: u64,
    pub savings_max: u64,
}

#[derive(Debug, Serialize)]
pub struct RuleRollup {
    pub rule: String,
    pub count: usize,
    pub savings_min: u64,
    pub savings_max: u64,
}

#[derive(Debug, Serialize)]
pub struct SecretFile {
    pub path: String,
    pub secret_count: usize,
    pub kinds: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Totals {
    pub files_scanned: u64,
    pub context_files: usize,
    pub context_tokens: u64,
    pub savings_min: u64,
    pub savings_max: u64,
}

#[derive(Debug, Serialize)]
pub struct ProjectReport {
    pub root: String,
    pub profiles: Profiles,
    pub mcp_configs: Vec<McpConfigEntry>,
    pub heaviest: Vec<HeavyFile>,
    pub findings_by_rule: Vec<RuleRollup>,
    pub secret_files: Vec<SecretFile>,
    pub totals: Totals,
    pub warnings: Vec<String>,
}

impl ProjectReport {
    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.profiles
            .always
            .files
            .iter()
            .chain(self.profiles.on_invocation.files.iter())
            .chain(self.profiles.on_demand.files.iter())
            .chain(self.profiles.docs.files.iter())
    }
}

/// The `--fail-on` decision: does any finding sit at or above `threshold`?
pub fn exceeds_severity(report: &ProjectReport, threshold: &str) -> bool {
    let bar = severity_rank(threshold);
    report
        .files()
        .any(|f| f.findings.iter().any(|x| severity_rank(&x.severity) <= bar))
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

struct Candidate {
    rel: String,
    abs: PathBuf,
    kind: Kind,
}

pub fn analyze(root: &Path, opts: &ProjectOptions) -> ProjectReport {
    let mut warnings = Vec::new();
    let (files_scanned, candidates) = discover(root, &mut warnings);

    let (mcp_cands, file_cands): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|c| matches!(c.kind, Kind::McpConfig));

    let mcp_configs = analyze_mcp(&mcp_cands, &mut warnings);

    // Warm the o200k encoder once so parallel workers do not all pay the
    // OnceLock construction race.
    let _ = tokenize::count(TokProvider::OpenAi, "warm");
    let (entries, mut file_warnings) = analyze_files(&file_cands, opts);
    warnings.append(&mut file_warnings);

    rollup(
        root,
        files_scanned,
        entries,
        mcp_configs,
        opts.top,
        warnings,
    )
}

fn discover(root: &Path, warnings: &mut Vec<String>) -> (u64, Vec<Candidate>) {
    let mut files_scanned: u64 = 0;
    let mut candidates = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false) // agent context lives in dotfiles (.claude, .cursor, ...)
        .follow_links(false)
        .require_git(false) // honor .gitignore even when DIR is not a checkout
        .git_global(false) // machine-local excludes would make runs non-reproducible
        .git_exclude(false)
        .filter_entry(|e| {
            let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            !(dir
                && e.depth() > 0
                && matches!(
                    e.file_name().to_str(),
                    Some(".git") | Some("node_modules") | Some("target")
                ))
        })
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                warnings.push(format!("walk error: {e}"));
                continue;
            }
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        files_scanned += 1;
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        if let Some(kind) = classify(rel) {
            candidates.push(Candidate {
                rel: rel.to_string_lossy().replace('\\', "/"),
                abs: entry.into_path(),
                kind,
            });
        }
    }
    // Walk order is filesystem-dependent; sort for a stable report.
    candidates.sort_by(|a, b| a.rel.cmp(&b.rel));
    (files_scanned, candidates)
}

fn analyze_mcp(cands: &[Candidate], warnings: &mut Vec<String>) -> Vec<McpConfigEntry> {
    let mut out = Vec::new();
    for c in cands {
        let text = match fs::read_to_string(&c.abs) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", c.rel));
                continue;
            }
        };
        match mcp::analyze(&text, CoreProvider::OpenAi) {
            Ok(analysis) => out.push(McpConfigEntry {
                path: c.rel.clone(),
                servers: analysis.totals.servers,
                cold_tokens: analysis.totals.cold,
            }),
            // Settings files routinely exist without an MCP section; that is
            // an expected skip, not a problem (same posture as `tolkin scan`).
            Err(e) if e.contains("no MCP servers") || e.contains("no servers in it") => {}
            Err(e) => warnings.push(format!("could not analyze {}: {e}", c.rel)),
        }
    }
    out
}

/// Token counting dominates the runtime. The tokenizer is a shared &'static
/// behind OnceLock and counting is read-only, so contiguous chunks run on
/// scoped threads; concatenating results in chunk order keeps the output
/// deterministic.
fn analyze_files(cands: &[Candidate], opts: &ProjectOptions) -> (Vec<FileEntry>, Vec<String>) {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8);
    if threads <= 1 || cands.len() < 16 {
        return process_chunk(cands, opts);
    }
    let chunk_size = cands.len().div_ceil(threads);
    let opts = *opts;
    let results: Vec<(Vec<FileEntry>, Vec<String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = cands
            .chunks(chunk_size)
            .map(|chunk| scope.spawn(move || process_chunk(chunk, &opts)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("analysis worker panicked"))
            .collect()
    });
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for (e, w) in results {
        entries.extend(e);
        warnings.extend(w);
    }
    (entries, warnings)
}

fn process_chunk(cands: &[Candidate], opts: &ProjectOptions) -> (Vec<FileEntry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for c in cands {
        match fs::metadata(&c.abs) {
            Ok(m) if m.len() > opts.max_file_bytes => {
                warnings.push(format!(
                    "skipped {} ({} bytes exceeds --max-file-bytes {})",
                    c.rel,
                    m.len(),
                    opts.max_file_bytes
                ));
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                warnings.push(format!("could not stat {}: {e}", c.rel));
                continue;
            }
        }
        let bytes = match fs::read(&c.abs) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", c.rel));
                continue;
            }
        };
        // Non-UTF-8 means binary (images inside skill dirs are normal). Skip
        // quietly rather than warn-spamming.
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        match c.kind {
            Kind::Plain { class, profile } => {
                let Some(tokens) = count_tokens(&text, &c.rel, &mut warnings) else {
                    continue;
                };
                let (secret_count, secret_kinds) = secret_stats(&text);
                entries.push(FileEntry {
                    path: c.rel.clone(),
                    class: class.to_string(),
                    profile,
                    tokens,
                    findings: audit_findings(&text, tokens, opts.experimental),
                    secret_count,
                    secret_kinds,
                    skill_name: None,
                    skill_description: None,
                });
            }
            Kind::Skill => {
                let split = split_skill(&text);
                let Some(body_tokens) = count_tokens(&split.body, &c.rel, &mut warnings) else {
                    continue;
                };
                let fm_tokens = match &split.frontmatter {
                    Some(fm) => match count_tokens(fm, &c.rel, &mut warnings) {
                        Some(n) => n,
                        None => continue,
                    },
                    None => 0,
                };
                // Audit and secret-scan the whole file; the results ride on
                // the body entry so per-path rollups stay additive.
                let findings = audit_findings(&text, fm_tokens + body_tokens, opts.experimental);
                let (secret_count, secret_kinds) = secret_stats(&text);
                if split.frontmatter.is_some() {
                    entries.push(FileEntry {
                        path: c.rel.clone(),
                        class: "skill-frontmatter".to_string(),
                        profile: Profile::Always,
                        tokens: fm_tokens,
                        findings: Vec::new(),
                        secret_count: 0,
                        secret_kinds: Vec::new(),
                        skill_name: split.name.clone(),
                        skill_description: split.description.clone(),
                    });
                }
                entries.push(FileEntry {
                    path: c.rel.clone(),
                    class: "skill-body".to_string(),
                    profile: Profile::OnInvocation,
                    tokens: body_tokens,
                    findings,
                    secret_count,
                    secret_kinds,
                    skill_name: None,
                    skill_description: None,
                });
            }
            Kind::McpConfig => unreachable!("MCP configs are partitioned out before file analysis"),
        }
    }
    (entries, warnings)
}

fn count_tokens(text: &str, rel: &str, warnings: &mut Vec<String>) -> Option<u64> {
    match tokenize::count(TokProvider::OpenAi, text) {
        Ok(n) => Some(n as u64),
        Err(e) => {
            warnings.push(format!("could not tokenize {rel}: {e}"));
            None
        }
    }
}

fn audit_findings(text: &str, tokens: u64, experimental: bool) -> Vec<FindingSummary> {
    audit::audit(
        text,
        &AuditOptions {
            input_tokens: Some(tokens),
            include_experimental: experimental,
        },
    )
    .findings
    .into_iter()
    .map(|f| FindingSummary {
        rule: f.rule,
        severity: f.severity,
        savings_min: f.savings_min,
        savings_max: f.savings_max,
    })
    .collect()
}

/// Privacy rule (absolute): keep only counts and kind names. Values, byte
/// spans, and redacted text are dropped right here and never reach a struct.
fn secret_stats(text: &str) -> (usize, Vec<String>) {
    let result = redact::redact(text, &RedactOptions::default());
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
    (count, kinds)
}

fn rollup(
    root: &Path,
    files_scanned: u64,
    entries: Vec<FileEntry>,
    mcp_configs: Vec<McpConfigEntry>,
    top: usize,
    warnings: Vec<String>,
) -> ProjectReport {
    let mut by_path: BTreeMap<String, HeavyFile> = BTreeMap::new();
    let mut by_rule: BTreeMap<String, RuleRollup> = BTreeMap::new();
    let mut secret_files = Vec::new();
    let mut savings_min = 0u64;
    let mut savings_max = 0u64;

    for e in &entries {
        let heavy = by_path.entry(e.path.clone()).or_insert_with(|| HeavyFile {
            path: e.path.clone(),
            tokens: 0,
            findings: 0,
            savings_min: 0,
            savings_max: 0,
        });
        heavy.tokens += e.tokens;
        heavy.findings += e.findings.len();
        for f in &e.findings {
            heavy.savings_min += f.savings_min;
            heavy.savings_max += f.savings_max;
            savings_min += f.savings_min;
            savings_max += f.savings_max;
            let rule = by_rule.entry(f.rule.clone()).or_insert_with(|| RuleRollup {
                rule: f.rule.clone(),
                count: 0,
                savings_min: 0,
                savings_max: 0,
            });
            rule.count += 1;
            rule.savings_min += f.savings_min;
            rule.savings_max += f.savings_max;
        }
        if e.secret_count > 0 {
            secret_files.push(SecretFile {
                path: e.path.clone(),
                secret_count: e.secret_count,
                kinds: e.secret_kinds.clone(),
            });
        }
    }
    let context_files = by_path.len();

    let mut profiles = Profiles::default();
    for e in entries {
        let slot = match e.profile {
            Profile::Always => &mut profiles.always,
            Profile::OnInvocation => &mut profiles.on_invocation,
            Profile::OnDemand => &mut profiles.on_demand,
            Profile::Docs => &mut profiles.docs,
        };
        slot.tokens += e.tokens;
        slot.files.push(e);
    }
    // MCP tool definitions reach the model at session start, so their cold
    // cost is an always-bucket weight even though configs are not text files.
    profiles.always.tokens += mcp_configs.iter().map(|m| m.cold_tokens).sum::<u64>();

    let mut heaviest: Vec<HeavyFile> = by_path.into_values().collect();
    heaviest.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.path.cmp(&b.path)));
    heaviest.truncate(top);

    let mut findings_by_rule: Vec<RuleRollup> = by_rule.into_values().collect();
    findings_by_rule.sort_by(|a, b| b.savings_max.cmp(&a.savings_max).then(a.rule.cmp(&b.rule)));

    let context_tokens = profiles.always.tokens
        + profiles.on_invocation.tokens
        + profiles.on_demand.tokens
        + profiles.docs.tokens;

    ProjectReport {
        root: root.display().to_string(),
        profiles,
        mcp_configs,
        heaviest,
        findings_by_rule,
        secret_files,
        totals: Totals {
            files_scanned,
            context_files,
            context_tokens,
            savings_min,
            savings_max,
        },
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-project-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn opts() -> ProjectOptions {
        ProjectOptions {
            experimental: false,
            max_file_bytes: 2_000_000,
            top: 10,
        }
    }

    fn entry(
        path: &str,
        profile: Profile,
        tokens: u64,
        findings: Vec<FindingSummary>,
    ) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            class: "doc".to_string(),
            profile,
            tokens,
            findings,
            secret_count: 0,
            secret_kinds: Vec::new(),
            skill_name: None,
            skill_description: None,
        }
    }

    fn finding(rule: &str, severity: &str, min: u64, max: u64) -> FindingSummary {
        FindingSummary {
            rule: rule.to_string(),
            severity: severity.to_string(),
            savings_min: min,
            savings_max: max,
        }
    }

    #[test]
    fn classifier_puts_instruction_files_in_always_at_any_depth() {
        for p in [
            "CLAUDE.md",
            "apps/web/CLAUDE.md",
            "deep/nested/AGENTS.md",
            "GEMINI.md",
        ] {
            match classify(Path::new(p)) {
                Some(Kind::Plain { profile, .. }) => assert_eq!(profile, Profile::Always, "{p}"),
                other => panic!("{p}: {other:?}"),
            }
        }
        assert_eq!(
            classify(Path::new(".cursor/rules/style.mdc")),
            Some(Kind::Plain {
                class: "cursor-rule",
                profile: Profile::Always
            })
        );
        assert_eq!(
            classify(Path::new(".github/copilot-instructions.md")),
            Some(Kind::Plain {
                class: "copilot-instructions",
                profile: Profile::Always
            })
        );
    }

    #[test]
    fn classifier_splits_invocation_and_demand_classes() {
        assert_eq!(
            classify(Path::new(".claude/skills/deploy/SKILL.md")),
            Some(Kind::Skill)
        );
        assert_eq!(classify(Path::new("anywhere/SKILL.md")), Some(Kind::Skill));
        assert_eq!(
            classify(Path::new(".claude/commands/review.md")),
            Some(Kind::Plain {
                class: "command",
                profile: Profile::OnInvocation
            })
        );
        assert_eq!(
            classify(Path::new(".claude/agents/tester.md")),
            Some(Kind::Plain {
                class: "agent",
                profile: Profile::OnInvocation
            })
        );
        assert_eq!(
            classify(Path::new(".codex/prompts/fix.md")),
            Some(Kind::Plain {
                class: "codex-prompt",
                profile: Profile::OnInvocation
            })
        );
        assert_eq!(
            classify(Path::new(".claude/skills/deploy/references/notes.md")),
            Some(Kind::Plain {
                class: "skill-resource",
                profile: Profile::OnDemand
            })
        );
        assert_eq!(
            classify(Path::new("prompts/summarize.md")),
            Some(Kind::Plain {
                class: "prompt-doc",
                profile: Profile::OnDemand
            })
        );
    }

    #[test]
    fn classifier_docs_only_at_root_and_skips_plain_source_docs() {
        assert_eq!(
            classify(Path::new("README.md")),
            Some(Kind::Plain {
                class: "doc",
                profile: Profile::Docs
            })
        );
        assert_eq!(classify(Path::new("src/foo.md")), None);
        assert_eq!(classify(Path::new("src/main.rs")), None);
    }

    #[test]
    fn classifier_recognizes_mcp_configs() {
        for p in [
            ".mcp.json",
            "apps/web/.mcp.json",
            ".cursor/mcp.json",
            ".vscode/mcp.json",
            ".zed/settings.json",
            ".claude/settings.json",
        ] {
            assert_eq!(classify(Path::new(p)), Some(Kind::McpConfig), "{p}");
        }
        assert_eq!(classify(Path::new("settings.json")), None);
    }

    #[test]
    fn skill_split_with_frontmatter_extracts_name_and_description() {
        let text = "---\nname: deploy\ndescription: \"Ship the app safely\"\n---\n# Deploy\n\nSteps here.\n";
        let split = split_skill(text);
        assert_eq!(
            split.frontmatter.as_deref(),
            Some("name: deploy\ndescription: \"Ship the app safely\"")
        );
        assert_eq!(split.name.as_deref(), Some("deploy"));
        assert_eq!(split.description.as_deref(), Some("Ship the app safely"));
        assert!(split.body.starts_with("# Deploy"));
        assert!(split.body.contains("Steps here."));
    }

    #[test]
    fn skill_split_without_frontmatter_is_all_body() {
        let text = "# Deploy\n\nNo frontmatter here.\n";
        let split = split_skill(text);
        assert!(split.frontmatter.is_none());
        assert!(split.name.is_none());
        assert_eq!(split.body, text);

        // An opening fence with no closing fence is not frontmatter.
        let dangling = split_skill("---\nname: x\nno close");
        assert!(dangling.frontmatter.is_none());
        assert!(dangling.body.starts_with("---"));
    }

    #[test]
    fn gitignore_is_respected() {
        let root = tmp("gitignore");
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        fs::write(root.join("CLAUDE.md"), "# Rules\n\nUse bun.\n").unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join("ignored/CLAUDE.md"), "# Should not appear\n").unwrap();

        let report = analyze(&root, &opts());
        let paths: Vec<&str> = report.files().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"CLAUDE.md"), "{paths:?}");
        assert!(
            !paths.iter().any(|p| p.starts_with("ignored/")),
            "{paths:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn analyze_classifies_splits_skills_and_folds_mcp_into_always() {
        let root = tmp("analyze");
        fs::write(root.join("CLAUDE.md"), "# Project rules\n\nUse bun.\n").unwrap();
        fs::write(root.join("README.md"), "# Readme\n\nHello.\n").unwrap();
        fs::create_dir_all(root.join(".claude/skills/deploy/references")).unwrap();
        fs::write(
            root.join(".claude/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Ship it\n---\n# Deploy\n\nLong body text goes here.\n",
        )
        .unwrap();
        fs::write(
            root.join(".claude/skills/deploy/references/notes.md"),
            "# Notes\n\nReference material.\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".claude/commands")).unwrap();
        fs::write(root.join(".claude/commands/go.md"), "Run the thing.\n").unwrap();
        fs::write(
            root.join(".mcp.json"),
            r#"{ "mcpServers": { "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] } } }"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/notes.md"), "not agent context\n").unwrap();

        let report = analyze(&root, &opts());

        let always: Vec<&str> = report
            .profiles
            .always
            .files
            .iter()
            .map(|f| f.class.as_str())
            .collect();
        assert!(always.contains(&"claude-md"), "{always:?}");
        assert!(always.contains(&"skill-frontmatter"), "{always:?}");
        let fm = report
            .profiles
            .always
            .files
            .iter()
            .find(|f| f.class == "skill-frontmatter")
            .unwrap();
        assert_eq!(fm.skill_name.as_deref(), Some("deploy"));
        assert!(fm.tokens > 0);

        let invocation: Vec<&str> = report
            .profiles
            .on_invocation
            .files
            .iter()
            .map(|f| f.class.as_str())
            .collect();
        assert!(invocation.contains(&"skill-body"), "{invocation:?}");
        assert!(invocation.contains(&"command"), "{invocation:?}");
        assert_eq!(report.profiles.on_demand.files.len(), 1);
        assert_eq!(report.profiles.docs.files.len(), 1);
        assert!(!report.files().any(|f| f.path == "src/notes.md"));

        assert_eq!(report.mcp_configs.len(), 1);
        assert_eq!(report.mcp_configs[0].servers, 1);
        assert!(report.mcp_configs[0].cold_tokens > 0);
        let always_file_tokens: u64 = report.profiles.always.files.iter().map(|f| f.tokens).sum();
        assert_eq!(
            report.profiles.always.tokens,
            always_file_tokens + report.mcp_configs[0].cold_tokens
        );

        // SKILL.md counts once toward context files even with two entries.
        assert_eq!(report.totals.context_files, 5);
        assert_eq!(
            report.totals.context_tokens,
            report.profiles.always.tokens
                + report.profiles.on_invocation.tokens
                + report.profiles.on_demand.tokens
                + report.profiles.docs.tokens
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rollup_totals_add_up() {
        let entries = vec![
            entry(
                "CLAUDE.md",
                Profile::Always,
                100,
                vec![finding("json-verbosity", "medium", 10, 20)],
            ),
            entry("a/SKILL.md", Profile::Always, 30, vec![]),
            entry(
                "a/SKILL.md",
                Profile::OnInvocation,
                200,
                vec![finding("json-verbosity", "medium", 5, 9)],
            ),
            entry("README.md", Profile::Docs, 50, vec![]),
        ];
        let mcp = vec![McpConfigEntry {
            path: ".mcp.json".to_string(),
            servers: 2,
            cold_tokens: 1_000,
        }];
        let report = rollup(Path::new("/r"), 10, entries, mcp, 10, Vec::new());
        assert_eq!(report.profiles.always.tokens, 100 + 30 + 1_000);
        assert_eq!(report.profiles.on_invocation.tokens, 200);
        assert_eq!(report.profiles.docs.tokens, 50);
        assert_eq!(report.totals.context_tokens, 1_380);
        assert_eq!(report.totals.context_files, 3);
        assert_eq!(report.totals.savings_min, 15);
        assert_eq!(report.totals.savings_max, 29);
        // SKILL.md aggregates across its two entries in the heaviest list.
        assert_eq!(report.heaviest[0].path, "a/SKILL.md");
        assert_eq!(report.heaviest[0].tokens, 230);
        assert_eq!(report.findings_by_rule.len(), 1);
        assert_eq!(report.findings_by_rule[0].count, 2);
        assert_eq!(report.findings_by_rule[0].savings_max, 29);
    }

    #[test]
    fn fail_on_compares_against_the_threshold() {
        let entries = vec![entry(
            "CLAUDE.md",
            Profile::Always,
            100,
            vec![finding("near-duplicate-paragraphs", "medium", 10, 20)],
        )];
        let report = rollup(Path::new("/r"), 1, entries, Vec::new(), 10, Vec::new());
        assert!(!exceeds_severity(&report, "high"));
        assert!(exceeds_severity(&report, "medium"));
        assert!(exceeds_severity(&report, "low"));

        let clean = rollup(Path::new("/r"), 0, Vec::new(), Vec::new(), 10, Vec::new());
        assert!(!exceeds_severity(&clean, "low"));
    }

    #[test]
    fn report_serializes_to_the_documented_json_shape() {
        let entries = vec![entry("CLAUDE.md", Profile::Always, 100, vec![])];
        let report = rollup(Path::new("/r"), 3, entries, Vec::new(), 10, Vec::new());
        let v = serde_json::to_value(&report).unwrap();
        for key in [
            "root",
            "profiles",
            "mcp_configs",
            "heaviest",
            "findings_by_rule",
            "secret_files",
            "totals",
            "warnings",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        let file = &v["profiles"]["always"]["files"][0];
        for key in [
            "path",
            "class",
            "profile",
            "tokens",
            "findings",
            "secret_count",
            "secret_kinds",
        ] {
            assert!(file.get(key).is_some(), "missing file key {key}");
        }
        assert_eq!(file["profile"], "always");
        assert_eq!(v["totals"]["files_scanned"], 3);
    }

    #[test]
    fn max_file_bytes_skips_oversized_files_with_a_warning() {
        let root = tmp("maxbytes");
        fs::write(root.join("CLAUDE.md"), "x".repeat(4_096)).unwrap();
        fs::write(root.join("AGENTS.md"), "small file\n").unwrap();
        let report = analyze(
            &root,
            &ProjectOptions {
                experimental: false,
                max_file_bytes: 1_024,
                top: 10,
            },
        );
        let paths: Vec<&str> = report.files().map(|f| f.path.as_str()).collect();
        assert!(!paths.contains(&"CLAUDE.md"), "{paths:?}");
        assert!(paths.contains(&"AGENTS.md"), "{paths:?}");
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("CLAUDE.md") && w.contains("--max-file-bytes")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn secret_stats_carry_kinds_and_counts_only() {
        let (count, kinds) =
            secret_stats("api key: export OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwx123456\n");
        assert!(count >= 1);
        assert!(kinds.iter().all(|k| !k.contains("sk-proj")));
    }
}
