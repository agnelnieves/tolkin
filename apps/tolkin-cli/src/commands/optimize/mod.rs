//! `tolkin optimize`: deterministic optimization summary plus an opt-in
//! "model advisory (local)" layer (narration and skill lint) served by a
//! loopback sidecar.
//!
//! Identity rules (MLX-RESEARCH-FINDINGS.md sections 3 to 6, PRIVACY.md
//! "The local model sidecar (optimize)"):
//! - the deterministic skeleton is identical with or without a sidecar,
//! - model output is a distinct display class, never a measurement tier,
//!   never summed with tier numbers, and it never edits files,
//! - the model path is hard-off under CI=true or TOLKIN_NO_SIDECAR=1,
//! - consent is asked interactively, once, and never non-interactively,
//! - file content passes the always-on secret redaction before any prompt.

mod gates;
mod prompts;
mod skeleton;

use std::collections::BTreeMap;
use std::env;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use serde_json::json;
use tolkin_core::redact::RedactOptions;

use crate::ledger::{self, Config};
use crate::project::{self, ProjectOptions, ProjectReport};
use crate::sidecar::{self, ChatRequest, ProbeReport, Sidecar};

#[derive(Args, Debug)]
pub struct OptimizeArgs {
    /// Which advisory tasks to plan or run.
    #[arg(long, default_value = "all", value_parser = ["narrate", "skills", "all"])]
    pub task: String,

    /// Plan only: skeleton, probes, consent state, files, time estimate. Zero model calls.
    #[arg(long)]
    pub dry_run: bool,

    /// With --dry-run, also print the exact prompts that would be sent.
    #[arg(long)]
    pub show_prompts: bool,

    /// Emit JSON instead of the human report.
    #[arg(long)]
    pub json: bool,

    /// Sidecar base URL override (loopback enforced).
    #[arg(long)]
    pub base_url: Option<String>,

    /// Sidecar model id override.
    #[arg(long)]
    pub model: Option<String>,

    /// Cap on skill files sent to the model.
    #[arg(long, default_value_t = 8)]
    pub max_files: usize,
}

/// Consent state for the local-model layer, as reported in --json.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Consent {
    Granted,
    Declined,
    NotAsked,
    Unavailable,
}

impl Consent {
    fn as_str(self) -> &'static str {
        match self {
            Consent::Granted => "granted",
            Consent::Declined => "declined",
            Consent::NotAsked => "not_asked",
            Consent::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy)]
struct Tasks {
    narrate: bool,
    skills: bool,
}

fn tasks_from(arg: &str) -> Tasks {
    match arg {
        "narrate" => Tasks {
            narrate: true,
            skills: false,
        },
        "skills" => Tasks {
            narrate: false,
            skills: true,
        },
        _ => Tasks {
            narrate: true,
            skills: true,
        },
    }
}

/// The model path is hard-off in CI and under TOLKIN_NO_SIDECAR=1
/// (PRIVACY.md; findings section 6, adjudication 4). CI truthiness mirrors
/// the ledger's semantics: set, non-empty, not "0", not "false".
fn model_path_disabled() -> bool {
    ci_truthy() || sidecar::disabled_by_env()
}

fn ci_truthy() -> bool {
    match env::var("CI") {
        Ok(v) => {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        }
        Err(_) => false,
    }
}

/// Narration plan: the prompt that would be (or is) sent, plus its token
/// count for the time estimate.
struct NarratePlan {
    user: String,
    prompt_tokens: usize,
}

/// One skill file ready for lint: redacted content baked into the user
/// prompt. `redacted` is kept for the span-grounding gate.
struct SkillFilePlan {
    path: String,
    user: String,
    prompt_tokens: usize,
    redacted: String,
}

struct SkillsPlan {
    files: Vec<SkillFilePlan>,
    /// How many discovered files fell past the --max-files cap.
    capped: usize,
}

struct Estimate {
    per_task: BTreeMap<&'static str, u64>,
    total_seconds: u64,
    basis: String,
}

/// Aggregated model output for one run. Distinct display class: this struct
/// never feeds tier math and is rendered only with the advisory label.
struct Advisory {
    model: String,
    backend: &'static str,
    narration: Option<String>,
    narration_verified: bool,
    /// Human-facing note when narration is absent (discarded or unavailable).
    narration_note: Option<String>,
    skill_lint: Vec<SkillLintEntry>,
}

struct SkillLintEntry {
    file: String,
    summary: String,
    findings: Vec<gates::SkillFinding>,
    dropped_findings: usize,
}

pub fn run(args: OptimizeArgs, yes: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = cwd.canonicalize().unwrap_or(cwd);

    // Step 1: the deterministic skeleton, always built, sidecar or not.
    let opts = ProjectOptions {
        experimental: false,
        max_file_bytes: 2_000_000,
        top: 5,
    };
    let report = project::analyze(&root, &opts);
    let skel = skeleton::build(&report);
    let skeleton_json = skel.to_compact_json();

    let tasks = tasks_from(&args.task);
    let mut warnings: Vec<String> = Vec::new();

    // Step 2: sidecar resolution. CLI flags beat config beats default
    // loopback probes. Detection is skipped entirely when disabled.
    let cfg = ledger::load_config();
    let disabled = model_path_disabled();
    let probe = if disabled {
        ProbeReport::default()
    } else {
        let base = args
            .base_url
            .clone()
            .or_else(|| cfg.as_ref().and_then(|c| c.sidecar_base_url.clone()));
        let model = args
            .model
            .clone()
            .or_else(|| cfg.as_ref().and_then(|c| c.sidecar_model.clone()));
        sidecar::detect(base.as_deref(), model.as_deref())
    };
    if let Some(w) = &probe.model_warning {
        warnings.push(w.clone());
    }

    // Step 3: consent. Asked at most once, only interactively.
    let consent = resolve_consent(&args, yes, &probe, cfg);

    // Step 4: plans and the time estimate. Building them makes zero model
    // calls; the dry run and the JSON estimate need them either way.
    let narrate_plan = tasks
        .narrate
        .then(|| build_narrate_plan(&skeleton_json));
    let skills_plan = tasks
        .skills
        .then(|| build_skills_plan(&root, &report, args.max_files, &mut warnings));
    let estimate = build_estimate(narrate_plan.as_ref(), skills_plan.as_ref());

    let ctx = RenderCtx {
        args: &args,
        root: &root,
        skel: &skel,
        probe: &probe,
        consent,
        estimate: &estimate,
        narrate_plan: narrate_plan.as_ref(),
        skills_plan: skills_plan.as_ref(),
        warnings: &warnings,
        disabled,
    };

    if args.json {
        if consent == Consent::NotAsked {
            eprintln!("hint: local model detected; run tolkin optimize in an interactive terminal to grant or decline consent (or re-run tolkin init)");
        }
        let advisory = maybe_run_live(&ctx, &skeleton_json);
        print_json(&ctx, advisory.as_ref())?;
        return Ok(());
    }
    human_flow(&ctx, &skeleton_json);
    Ok(())
}

/// Live calls happen if and only if: not a dry run, consent granted, and a
/// sidecar resolved. Every other outcome leaves the advisory absent.
fn maybe_run_live(ctx: &RenderCtx, skeleton_json: &str) -> Option<Advisory> {
    if ctx.args.dry_run || ctx.consent != Consent::Granted {
        return None;
    }
    let sc = ctx.probe.sidecar.as_ref()?;
    Some(run_advisory(
        sc,
        skeleton_json,
        ctx.narrate_plan,
        ctx.skills_plan,
    ))
}

fn resolve_consent(
    args: &OptimizeArgs,
    yes: bool,
    probe: &ProbeReport,
    cfg: Option<Config>,
) -> Consent {
    if probe.sidecar.is_none() {
        return Consent::Unavailable;
    }
    match cfg.as_ref().and_then(|c| c.consent_local_model) {
        Some(true) => Consent::Granted,
        Some(false) => Consent::Declined,
        None => {
            let interactive =
                std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            // Never ask non-interactively, under --yes (which suppresses,
            // never auto-consents), in JSON mode, or during a dry run.
            if !interactive || yes || args.json || args.dry_run {
                return Consent::NotAsked;
            }
            let granted = ask_consent();
            persist_consent(cfg, granted);
            if granted {
                Consent::Granted
            } else {
                Consent::Declined
            }
        }
    }
}

/// One plain y/N question, onboard style. Defaults to no.
fn ask_consent() -> bool {
    use std::io::{BufRead, Write};
    {
        let mut out = std::io::stdout().lock();
        let _ = write!(
            out,
            "Allow tolkin to send the content of instruction and skill files it discovers to a local model server on this machine (127.0.0.1)? Nothing leaves your computer; output is advisory only. [y/N] "
        );
        let _ = out.flush();
    }
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn persist_consent(cfg: Option<Config>, granted: bool) {
    // Auto-onboarding creates the config before dispatch on a TTY; if it is
    // still missing, fall back to the onboarding defaults (ledger on, log
    // ingestion off) so the answer is not lost.
    let mut cfg = cfg.unwrap_or_else(|| Config::new(true, false));
    cfg.consent_local_model = Some(granted);
    if let Err(e) = ledger::save_config(&cfg) {
        eprintln!("warn: could not save consent: {e}");
    }
}

fn build_narrate_plan(skeleton_json: &str) -> NarratePlan {
    let user = prompts::narrate_user(skeleton_json);
    let prompt_tokens =
        prompts::estimate_tokens(prompts::NARRATE_SYSTEM) + prompts::estimate_tokens(&user);
    NarratePlan {
        user,
        prompt_tokens,
    }
}

/// Discover SKILL.md files from the project report (gitignore-aware walk,
/// including .claude/skills/*/SKILL.md), sorted for determinism and capped
/// at --max-files. Content is redacted before it enters any prompt.
fn build_skills_plan(
    root: &Path,
    report: &ProjectReport,
    max_files: usize,
    warnings: &mut Vec<String>,
) -> SkillsPlan {
    let mut paths: Vec<String> = Vec::new();
    for f in report.files() {
        if f.path.ends_with("SKILL.md") && !paths.contains(&f.path) {
            paths.push(f.path.clone());
        }
    }
    paths.sort();
    let capped = paths.len().saturating_sub(max_files);
    paths.truncate(max_files);

    let system_tokens = prompts::estimate_tokens(prompts::SKILL_SYSTEM);
    let mut files = Vec::new();
    for rel in paths {
        let abs: PathBuf = root.join(&rel);
        let Ok(text) = std::fs::read_to_string(&abs) else {
            warnings.push(format!("could not read {rel}; skipped"));
            continue;
        };
        // Redaction doctrine: the always-on secret redaction pass runs
        // before file content reaches the prompt builder (PRIVACY.md,
        // "The local model sidecar (optimize)").
        let redacted = tolkin_core::redact::redact(&text, &RedactOptions::default()).redacted_text;
        let (user, _input_truncated) = prompts::skill_user(&redacted);
        let prompt_tokens = system_tokens + prompts::estimate_tokens(&user);
        files.push(SkillFilePlan {
            path: rel,
            user,
            prompt_tokens,
            redacted,
        });
    }
    SkillsPlan { files, capped }
}

fn build_estimate(narrate: Option<&NarratePlan>, skills: Option<&SkillsPlan>) -> Estimate {
    let rates = prompts::chip_rates(prompts::detect_chip().as_deref());
    let mut per_task: BTreeMap<&'static str, u64> = BTreeMap::new();
    if let Some(n) = narrate {
        per_task.insert(
            "narrate",
            prompts::eta_seconds(n.prompt_tokens, prompts::NARRATE_MAX_TOKENS, &rates),
        );
    }
    if let Some(s) = skills {
        let secs: u64 = s
            .files
            .iter()
            .map(|f| prompts::eta_seconds(f.prompt_tokens, prompts::SKILL_MAX_TOKENS, &rates))
            .sum();
        per_task.insert("skills", secs);
    }
    let total_seconds = per_task.values().sum();
    let kind = if rates.measured { "measured" } else { "estimate" };
    let basis = format!(
        "{}: prefill {} tok/s, decode {} tok/s ({kind}); all times are estimates",
        rates.label, rates.prefill, rates.decode
    );
    Estimate {
        per_task,
        total_seconds,
        basis,
    }
}

// ---------------------------------------------------------------------------
// Live execution. Reached only with consent granted and a resolved sidecar.
// ---------------------------------------------------------------------------

fn run_advisory(
    sc: &Sidecar,
    skeleton_json: &str,
    narrate: Option<&NarratePlan>,
    skills: Option<&SkillsPlan>,
) -> Advisory {
    let mut adv = Advisory {
        model: sc.model.clone(),
        backend: sc.backend.label(),
        narration: None,
        narration_verified: false,
        narration_note: None,
        skill_lint: Vec::new(),
    };
    if let Some(plan) = narrate {
        match run_narrate(sc, skeleton_json, plan) {
            NarrateOutcome::Verified(text) => {
                adv.narration = Some(text);
                adv.narration_verified = true;
            }
            NarrateOutcome::Discarded => {
                adv.narration_note = Some(
                    "narration failed deterministic verification and was discarded".to_string(),
                );
            }
            NarrateOutcome::Unavailable(e) => {
                adv.narration_note = Some(format!("local model unavailable: {e}"));
            }
        }
    }
    if let Some(plan) = skills {
        adv.skill_lint = run_skills(sc, plan);
    }
    adv
}

enum NarrateOutcome {
    Verified(String),
    Discarded,
    Unavailable(String),
}

/// Narration with the number-faithfulness gate: one bounded retry naming the
/// violations, then discard. A dead server is a note, never an error exit.
fn run_narrate(sc: &Sidecar, skeleton_json: &str, plan: &NarratePlan) -> NarrateOutcome {
    let req = ChatRequest {
        system: prompts::NARRATE_SYSTEM,
        user: &plan.user,
        max_tokens: prompts::NARRATE_MAX_TOKENS,
        temperature: prompts::TEMPERATURE,
        json_schema: None,
    };
    let resp = match sc.chat(&req) {
        Ok(r) => r,
        Err(e) => return NarrateOutcome::Unavailable(e),
    };
    let violations = gates::number_violations(&resp.text, skeleton_json);
    if violations.is_empty() {
        return NarrateOutcome::Verified(resp.text);
    }
    let retry_system = prompts::narrate_retry_system(&violations);
    let retry = ChatRequest {
        system: &retry_system,
        user: &plan.user,
        max_tokens: prompts::NARRATE_MAX_TOKENS,
        temperature: prompts::TEMPERATURE,
        json_schema: None,
    };
    match sc.chat(&retry) {
        Err(e) => NarrateOutcome::Unavailable(e),
        Ok(r2) => {
            if gates::number_violations(&r2.text, skeleton_json).is_empty() {
                NarrateOutcome::Verified(r2.text)
            } else {
                NarrateOutcome::Discarded
            }
        }
    }
}

/// Per-file skill lint. Truncated responses get one retry with a tighter
/// ask; schema and span-grounding gates run on whatever comes back. A chat
/// failure records the reason and stops this task only.
fn run_skills(sc: &Sidecar, plan: &SkillsPlan) -> Vec<SkillLintEntry> {
    let schema = prompts::skill_schema();
    let mut entries = Vec::new();
    for f in &plan.files {
        let req = ChatRequest {
            system: prompts::SKILL_SYSTEM,
            user: &f.user,
            max_tokens: prompts::SKILL_MAX_TOKENS,
            temperature: prompts::TEMPERATURE,
            json_schema: Some(schema.clone()),
        };
        let mut resp = match sc.chat(&req) {
            Ok(r) => r,
            Err(e) => {
                entries.push(unavailable_entry(&f.path, &e));
                break;
            }
        };
        if sidecar::truncated(&resp, prompts::SKILL_MAX_TOKENS) {
            let retry_system = prompts::skill_retry_system();
            let retry = ChatRequest {
                system: &retry_system,
                user: &f.user,
                max_tokens: prompts::SKILL_MAX_TOKENS,
                temperature: prompts::TEMPERATURE,
                json_schema: Some(schema.clone()),
            };
            match sc.chat(&retry) {
                Ok(r2) => resp = r2,
                Err(e) => {
                    entries.push(unavailable_entry(&f.path, &e));
                    break;
                }
            }
        }
        entries.push(gate_skill_response(&f.path, &resp.text, &f.redacted));
    }
    entries
}

fn unavailable_entry(path: &str, reason: &str) -> SkillLintEntry {
    SkillLintEntry {
        file: path.to_string(),
        summary: format!("local model unavailable: {reason}"),
        findings: Vec::new(),
        dropped_findings: 0,
    }
}

fn gate_skill_response(path: &str, text: &str, redacted: &str) -> SkillLintEntry {
    let Some(obj) = sidecar::extract_json_object(text) else {
        return SkillLintEntry {
            file: path.to_string(),
            summary: "model output had no JSON object and was discarded".to_string(),
            findings: Vec::new(),
            dropped_findings: 0,
        };
    };
    match gates::validate_skill_payload(&obj, redacted) {
        Ok(v) => SkillLintEntry {
            file: path.to_string(),
            summary: v.summary,
            findings: v.findings,
            dropped_findings: v.dropped_findings,
        },
        Err(e) => SkillLintEntry {
            file: path.to_string(),
            summary: format!("model output was discarded: {e}"),
            findings: Vec::new(),
            dropped_findings: 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Rendering.
// ---------------------------------------------------------------------------

struct RenderCtx<'a> {
    args: &'a OptimizeArgs,
    root: &'a Path,
    skel: &'a skeleton::Skeleton,
    probe: &'a ProbeReport,
    consent: Consent,
    estimate: &'a Estimate,
    narrate_plan: Option<&'a NarratePlan>,
    skills_plan: Option<&'a SkillsPlan>,
    warnings: &'a [String],
    disabled: bool,
}

/// Stable machine shape. model_advisory is null whenever no live calls ran.
fn print_json(ctx: &RenderCtx, advisory: Option<&Advisory>) -> Result<()> {
    let probes: Vec<serde_json::Value> = ctx
        .probe
        .probed
        .iter()
        .map(|p| {
            json!({
                "url": p.url,
                "reachable": p.reachable,
                "backend": p.backend.label(),
                "models": p.models.len(),
                "error": p.error,
            })
        })
        .collect();
    let advisory_json = advisory.map(|a| {
        json!({
            "class": "model_advisory",
            "model": a.model,
            "backend": a.backend,
            "narration": a.narration,
            "narration_verified": a.narration_verified,
            "skill_lint": a.skill_lint.iter().map(|e| {
                json!({
                    "file": e.file,
                    "summary": e.summary,
                    "findings": e.findings,
                    "dropped_findings": e.dropped_findings,
                })
            }).collect::<Vec<_>>(),
        })
    });
    let doc = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "dry_run": ctx.args.dry_run,
        "skeleton": ctx.skel,
        "probes": probes,
        "consent": ctx.consent.as_str(),
        "estimate": {
            "per_task": ctx.estimate.per_task,
            "total_seconds": ctx.estimate.total_seconds,
            "basis": ctx.estimate.basis,
        },
        "model_advisory": advisory_json,
    });
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

/// Human flow. Prints incrementally so the time estimate always lands
/// before the first model call.
fn human_flow(ctx: &RenderCtx, skeleton_json: &str) {
    println!("Tolkin optimize: {}", ctx.root.display());
    println!("Nothing leaves this machine. Model output is advisory only.");
    println!();
    print!("{}", ctx.skel.render_human());
    for w in ctx.warnings {
        println!("warn: {w}");
    }
    if sidecar::remote_override_active() && !ctx.disabled {
        println!("warning: TOLKIN_SIDECAR_ALLOW_REMOTE=1 is set; sidecar traffic may leave this machine.");
    }
    println!();

    if ctx.args.dry_run {
        print_dry_run(ctx);
        return;
    }

    match ctx.consent {
        Consent::Unavailable => {
            if ctx.disabled {
                println!("local model path disabled (CI=true or TOLKIN_NO_SIDECAR=1); deterministic summary only.");
            } else {
                println!("no local model server detected; deeper local analysis is available.");
                println!("setup: run an OpenAI-compatible server on loopback (mlx-lm, Ollama, LM Studio, or llama-server), then re-run tolkin optimize.");
            }
        }
        Consent::Declined => {
            println!("local model consent declined; re-run tolkin init to change it.");
        }
        Consent::NotAsked => {
            println!("local model detected, but consent has not been asked; run tolkin optimize in an interactive terminal to decide (or re-run tolkin init).");
        }
        Consent::Granted => {
            print_eta_line(ctx);
            // Sidecar presence is implied by Granted; guard anyway.
            if let Some(sc) = ctx.probe.sidecar.as_ref() {
                let adv = run_advisory(sc, skeleton_json, ctx.narrate_plan, ctx.skills_plan);
                print_advisory(&adv);
            }
        }
    }
}

fn print_eta_line(ctx: &RenderCtx) {
    let mut parts: Vec<String> = Vec::new();
    if let Some(secs) = ctx.estimate.per_task.get("narrate") {
        parts.push(format!("narrate ~{secs}s"));
    }
    if let Some(secs) = ctx.estimate.per_task.get("skills") {
        let n = ctx.skills_plan.map_or(0, |p| p.files.len());
        let noun = if n == 1 { "file" } else { "files" };
        parts.push(format!("skills ~{secs}s ({n} {noun})"));
    }
    println!(
        "Local model time estimate ({}): {}, total ~{}s",
        ctx.estimate.basis,
        parts.join(", "),
        ctx.estimate.total_seconds
    );
    println!();
}

fn print_advisory(adv: &Advisory) {
    if adv.narration.is_some() || adv.narration_note.is_some() {
        println!("model advisory (local), model {}: narration", adv.model);
        match (&adv.narration, &adv.narration_note) {
            (Some(text), _) => {
                for line in text.lines() {
                    println!("  {line}");
                }
            }
            (None, Some(note)) => println!("  {note}"),
            (None, None) => {}
        }
        println!();
    }
    if !adv.skill_lint.is_empty() {
        println!("model advisory (local), model {}: skill lint", adv.model);
        for entry in &adv.skill_lint {
            println!("  {}: {}", entry.file, entry.summary);
            for f in &entry.findings {
                println!(
                    "    [{}] {}: \"{}\"",
                    severity_str(f.severity),
                    aspect_str(f.aspect),
                    f.evidence_quote
                );
                println!("      fix: {}", f.suggestion);
            }
            if entry.dropped_findings > 0 {
                println!(
                    "    dropped findings: {} (evidence not found verbatim in the file)",
                    entry.dropped_findings
                );
            }
        }
        println!();
    }
    println!("advisory only, never a measurement; verify before applying");
}

fn severity_str(s: gates::Severity) -> &'static str {
    match s {
        gates::Severity::High => "high",
        gates::Severity::Medium => "medium",
        gates::Severity::Low => "low",
    }
}

fn aspect_str(a: gates::Aspect) -> &'static str {
    match a {
        gates::Aspect::Trigger => "trigger",
        gates::Aspect::Verbosity => "verbosity",
        gates::Aspect::Examples => "examples",
        gates::Aspect::Structure => "structure",
    }
}

fn print_dry_run(ctx: &RenderCtx) {
    println!("Dry run plan");
    if ctx.disabled {
        println!("  sidecar probes: skipped (CI=true or TOLKIN_NO_SIDECAR=1)");
    } else if ctx.probe.probed.is_empty() {
        println!("  sidecar probes: none attempted");
    } else {
        println!("  sidecar probes");
        for p in &ctx.probe.probed {
            if p.reachable {
                let n = p.models.len();
                let noun = if n == 1 { "model" } else { "models" };
                println!(
                    "    {}  reachable, backend {}, {n} {noun}",
                    p.url,
                    p.backend.label()
                );
            } else {
                let err = p.error.as_deref().unwrap_or("unreachable");
                println!("    {}  unreachable ({err})", p.url);
            }
        }
    }
    println!("  consent: {}", ctx.consent.as_str());

    if let Some(plan) = ctx.narrate_plan {
        println!("  task narrate: prompt ~{} tokens", plan.prompt_tokens);
    }
    if let Some(plan) = ctx.skills_plan {
        let n = plan.files.len();
        let noun = if n == 1 { "file" } else { "files" };
        println!("  task skills: {n} {noun}");
        for f in &plan.files {
            println!("    {}  ~{} tokens", f.path, f.prompt_tokens);
        }
        if plan.capped > 0 {
            println!("    ({} more beyond the --max-files cap)", plan.capped);
        }
    }
    print!("  ");
    print_eta_line(ctx);
    if ctx.args.show_prompts {
        print_prompts(ctx);
    }
    println!("dry run: no model calls were made");
}

/// Exact prompts, post-redaction, exactly as they would be sent.
fn print_prompts(ctx: &RenderCtx) {
    println!("prompts (exact, after secret redaction)");
    if let Some(plan) = ctx.narrate_plan {
        println!("--- narrate system ---");
        println!("{}", prompts::NARRATE_SYSTEM);
        println!("--- narrate user ---");
        println!("{}", plan.user);
    }
    if let Some(plan) = ctx.skills_plan {
        if !plan.files.is_empty() {
            println!("--- skills system (identical for every file) ---");
            println!("{}", prompts::SKILL_SYSTEM);
            for f in &plan.files {
                println!("--- skills user: {} ---", f.path);
                println!("{}", f.user);
            }
        }
    }
}
