# Tokler Improvements Plan

The second planning document for Tokler. The original `PLAN.md` (same directory) remains the source of truth for the core architecture: the three workspaces, the Rust core, tokenization strategy, redaction, the MCP analyzer, the audit engine, and the honesty rules. This plan extends it with the measurement, experience, and distribution layer: proving the savings, tracking them locally, showing them beautifully, and putting Tokler in front of humans and agents everywhere.

Read order for a new contributor: `PLAN.md` (architecture) -> `PROGRESS.md` (what exists, bottom-up) -> this file (where it goes next).

## 0. Where we are (snapshot, 2026-06-10)

Everything below is built, tested, and published. See `PROGRESS.md` for the full log.

- `tokler-cli@0.3.0` live on npm with five platform packages (darwin arm64/x64, linux x64/arm64; windows pending). Publishes flow automatically on merge to main via OIDC Trusted Publishing.
- CLI commands: `count` (with Anthropic verify), `compare`, `viz`, `audit` (production-proven + experimental rules, format previews), `redact`, `cost`, `mcp` (CLI-swap + slim-profile recommendations with copy-pasteable snippets), `drift`, `scan` (local config discovery), `project` (repo-wide audit with load profiles and `--fail-on`).
- Web app: redaction-first analyzer, 3-provider counts, visualizer, cost calculator, audit panel, MCP analyzer, LLMLingua-2 compression preview.
- CI: tokler-ci (gates), tokler-publish (matrix build + npm), tokler-audit (PR comments on this repo), tokler-drift (weekly proxy-vs-ground-truth watchdog).

### Carried-over items from the original plan (still open)

1. Post-hydration browser pass over Phases 2b and 3 web surfaces.
2. Windows builds (npm platform package + workflow leg).
3. OSS extraction to a public repo; Homebrew tap depends on it.
4. Community MCP catalog as a refreshable external repo.
5. Hosted demo deployment.
6. Model2Vec semantic dedup pass (deferred Phase 3).
7. Gemma tokenizer bundling for the web (currently fetched from the HF Hub at runtime).
8. Pricing refresh cadence decision (PRICES_OBSERVED is manual).
9. ANTHROPIC_API_KEY repo secret so the drift watchdog can run for real.

Items 2 and 3 are absorbed into workstreams below; the rest remain on the backlog and are not blocked by anything here.

## 1. The honest answer: is Tokler system-agnostic today?

Mostly, with three concrete gaps. The core promise holds: the Rust core is pure and portable, tokenization is exact everywhere it claims to be, the WASM runs in any modern browser, and binaries are self-contained (no runtime downloads, no network). The gaps:

| Gap | Detail | Fix (workstream 1) |
|---|---|---|
| `scan` discovery paths are macOS-shaped | `Library/Application Support/Claude/...` and the VS Code settings path are macOS literals; Windows (`%APPDATA%`) and Linux (`~/.config`) locations for Claude Desktop and VS Code are missing | Per-OS path tables keyed on `cfg(target_os)` plus the `dirs` crate's platform dirs |
| No Windows binaries | npm covers darwin + linux only; the launcher prints "coming" | `windows-2022` leg in the publish matrix, `tokler-win32-x64` package; onig/esaxx C/C++ compile under MSVC, needs a CI proof |
| `scripts/bump-version.sh` is BSD-sed only | `sed -i ''` fails on GNU sed | Portable in-place edit (perl -pi or a tiny Rust xtask) |

Everything else checked clean: stdin/file handling, path joining via `PathBuf`, no shell-outs in scan (PATH probing is metadata-based), `directories`-style conventions not yet needed because nothing persists (that changes below, with the same crate solving it portably).

## 2. The Caveman question: positioning and proof

Research verified there are three distinct "caveman" artifacts, and the comparison must not conflate them:

- **wilpel/caveman-compression**: an input-side transformer (rewrites text telegraphically). Claims 40-58% on three ad-hoc examples, tokenizer unspecified, thin methodology.
- **JuliusBrussee/caveman** (the viral skill, the "65%" the user cites): an output-side advisor (tells the agent to answer tersely). Claims 65% average output reduction across 10 prompts. An independent 72-run benchmark (Guzik, Dev.to) found 9-21% against a fair baseline and showed an 85-token micro-prompt matching the 552-token skill. The claim depends on a permissive baseline.
- **jwiegley caveman skill**: rule-based compressor, no published numbers.

Tokler is a different animal: an analyzer and advisor whose flagship savings are **structural and configurational, measured exactly** (JSON minify deltas are exact; MCP tool-definition tokens are counted, not estimated; project context weight is tokenized with the real vocabularies). Caveman-style lossy rewriting is one tool in our belt (the LLMLingua-2 preview), not the product.

So we do not compete on a single headline percent. We compete on **rigor**: a public, reproducible benchmark with declared baselines, real tokenizer counts, injection costs included, and quality scoring on anything lossy. The viral tools have claims; Tokler will have a methodology. That is the stakeholder story and the website story.

## 3. Workstreams

### W1: Portability hardening (the agnostic claim, made true)

1. Per-OS scan catalog: every client config location specified for macOS, Linux, and Windows (`dirs::config_dir`, `dirs::data_dir`, explicit `%APPDATA%` handling). Tests per-OS via path-injection (the catalog already takes a home root in tests).
2. Windows build: `windows-2022` publish-matrix leg producing `tokler-win32-x64` (npm naming convention: `win32`), launcher map entry, `.exe` handling in the launcher and build script. Risk: `onig` under MSVC; fallback documented (swap CLI tokenizers crate to `default-features = false` + pure-Rust regex backend if MSVC fights back; counts are unaffected).
3. Portable bump script (perl-based in-place edit or `cargo xtask bump`).
4. CI proof: tokler-ci gains a windows job for `cargo test` on the CLI (the true portability gate).

Done when: `npx tokler-cli` works on a Windows runner and `tokler scan` finds Claude Desktop configs on all three OSes (path tables unit-tested).

### W2: Measurement foundation (the accuracy keystone)

Two data sources, one local ledger. Everything stays on the machine; nothing is transmitted, ever. This extends the privacy posture deliberately: from "no persistence" to "local-only, consented, resettable persistence." CLAUDE.md/AGENTS.md get this exact wording.

**2a. The ledger.** `directories::ProjectDirs::from("", "", "tokler")` data dir (platform-correct on all three OSes). Append-only JSONL plus a tiny index:

- One record per analyzing run (`project`, `mcp`, `scan`, `audit`): timestamp, command, project key (canonicalized root path), headline numbers (context weight by load profile, identified reclaimable min/max, MCP cold/slim/swap totals), tokler version, pricing version.
- Created only after onboarding consent (W4). `tokler stats --reset` wipes it. `TOKLER_NO_LEDGER=1` disables entirely (CI sets this implicitly when `CI=true`).

**2b. Real-usage ingestion (opt-in).** Parse local agent session logs for ground-truth spend:

- Claude Code: `~/.claude/projects/<slug>/*.jsonl`. Verified format: assistant records carry `message.usage` (input, output, cache_creation, cache_read), `message.model`, `requestId`, `timestamp`, and crucially a per-record `cwd` (attribute by `cwd`, never by reverse-engineering the directory slug). Dedup on (`message.id`, `requestId`) keeping the LAST record (streaming writes intermediate snapshots with growing output_tokens; first-wins undercounts, a known ccusage bug). Exclude `"model":"<synthetic>"`. Works for subscription and API sessions alike.
- Codex CLI: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` `token_count` events (verified). Second source, same reader trait.
- Gemini CLI later (format less stable); Cursor is server-side, out of scope.
- Cost math reuses `tokler-core::pricing` (cache-aware: fresh vs cache-read vs cache-write rates), labeled with `PRICES_OBSERVED`.

**2c. Three-tier savings accounting (the honesty design).** Every surfaced number belongs to exactly one tier, always labeled:

| Tier | Name | Definition | Confidence |
|---|---|---|---|
| 1 | Identified | What audit/mcp/project flag as reclaimable right now | Advisory estimate |
| 2 | Realized | Delta between ledger snapshots of the same project: always-loaded context weight at T0 minus today, times sessions observed since (sessions counted from real logs when ingested, else user-supplied rate) | Measured structure, estimated frequency |
| 3 | Measured | Actual spend trends from agent logs (tokens and dollars, cache-aware) | Ground truth |

The stakeholder headline is Tier 2 backed by Tier 3: "this repo's standing context dropped 6,640 tokens on 2026-06-03; across the 214 real sessions since, that is ~1.42M input tokens (~$4.27 at Sonnet rates) that were never sent." The formula ships in the docs; anyone can audit it.

### W3: `tokler stats` and the dashboard (the cool part)

**CLI command first, TUI on top.** `tokler stats` prints the current project's ledger summary (plain text table; `--json` for agents; `--global` for machine-wide). This works everywhere including CI and is what agents consume.

**The TUI**: bare `tokler` in a TTY (or `tokler stats --tui`) opens a Ratatui dashboard. Stack proven in-repo: the portfolio CLI already ships ratatui 0.29 + crossterm 0.28; we reuse its render patterns. Non-TTY invocations keep current behavior (help), so nothing breaks for scripts.

Tabs (v1, deliberately small):

1. **Project** (default): load-profile bars for the cwd repo, top heavy files, identified savings, realized-savings sparkline over snapshots, last audit findings count.
2. **Machine** (`--global` equivalent): all known projects ranked by standing context weight, total identified and realized across them, MCP configs status (slimmed yes/no per client).
3. **Spend** (only when log ingestion is on): daily token/cost bars from real usage, model breakdown, cache hit rate, the ccusage-style "API-equivalent" framing. A calendar heatmap (like the Claude Code stats screen) is the v1.1 polish, not v1; braille-cell heatmaps in ratatui are a solved pattern but cost layout time.

Inspirations measured against: ccusage (views, `--json`, the shareable compact mode), ccboard and claude-token-monitor (Ratatui prior art). Differentiator: nobody correlates *usage* with *structural savings*; Tokler owns that join.

`--compact` flag for a screenshot-friendly single-frame summary (the social/share artifact).

### W4: Onboarding and first-run preflight

First run (no ledger, TTY): a sub-5-second guided flow, plain prompts (dialoguer-style, no full TUI needed):

1. Banner + one-line promise: "Nothing leaves this machine."
2. Preflight checks with live ticks: tokenizers load, which agent CLIs are on PATH, MCP configs found (count per client), agent instruction files found in cwd, agent session logs detected (Claude Code / Codex).
3. Two consents: local savings ledger (yes/no), usage-log ingestion (yes/no, only offered when logs were detected).
4. A "here is what to run" card: `tokler scan`, `tokler project`, `tokler stats`, plus the one most relevant suggestion from preflight (e.g. "3 MCP configs found, 2 slimmable: run tokler scan").

Non-interactive: `--yes` accepts defaults (ledger on, ingestion off); `CI=true` skips onboarding entirely. `tokler init` re-runs it on demand. Config stored next to the ledger (`config.toml`: consents, default provider, session-rate fallback).

### W5: The benchmark (proof for the website and stakeholders)

A public, reproducible harness. Three tracks, because conflating them is how the space lies to itself:

1. **Structural (lossless/near-lossless)**: corpus of real-shaped fixtures (pretty JSON configs, HTML docs, duplicated-paragraph prompts, stack-trace logs). Metric: exact before/after tokens per provider vocabulary, zero quality question (lossless by construction). This is where Tokler's numbers are unbeatable because they are measurements, not claims.
2. **Configuration (MCP)**: the catalog corpus plus real public server manifests. Metric: tool-definition tokens cold/warm, slim and swap deltas, percent of a 200K window reclaimed. Compare against caveman-shrink (the caveman ecosystem's MCP middleware) on the same manifests.
3. **Lossy (compression)**: our LLMLingua-2 preview at 0.7/0.5/0.33 vs wilpel caveman scripts on the same inputs. Metrics: compression ratio AND quality. Quality scoring v1 is the Guzik method (extraction tasks with verifiable answers, automated fact checking; runnable with BYOK, off by default); we publish ratios always and quality runs when keys are provided. Every lossy number carries the RCT caveat (input savings can grow outputs).

Methodology rules (the differentiator, stated on the page): declared baselines, injection/prompt overhead counted against the technique, real tokenizer counts (o200k exact; Anthropic via count_tokens when keyed), N runs with variance, all scripts and fixtures in the repo, date-stamped pricing.

Deliverables: `benchmarks/` directory (fixtures + a Rust or bun runner emitting `results.json` + `RESULTS.md`), a `tokler-bench.yml` dispatch workflow, and a website page on tokler-web rendering `results.json` (static, client-side, consistent with the no-server posture). The website page doubles as the stakeholder deck source.

### W6: Distribution for humans and agents

**6a. Public companion repo** (the unlock for everything below): `tokler` distribution repo containing the skills, the plugin manifest, the GitHub Action, benchmark results mirror, and Homebrew formula. Product source can stay private; this repo is docs + glue + release artifacts. (Full OSS extraction remains on the backlog; this is the thin public surface that does not wait for it.)

**6b. Skills + plugin, dual channel from one layout** (verified conventions):

- Root `skills/<name>/SKILL.md` with frontmatter makes it `npx skills add <repo> --skill <name>` installable across ~71 agents (universal `.agents/skills/` plus per-agent dirs).
- `.claude-plugin/plugin.json` + `marketplace.json` in the same repo makes it a Claude Code plugin (`/plugin marketplace add`), with namespaced skills and optionally a `bin/` shim that puts tokler on the agent's PATH.
- v1 skills: `tokler-audit` (run project/audit, interpret JSON, prioritize by severity), `tokler-slim` (apply MCP slim snippets, re-run `tokler mcp` to verify the delta, report realized savings), `tokler-optimize` (the loop: audit -> apply safe fixes -> re-measure -> summarize with tier labels). The skills close the realized-savings loop: agents do not just read recommendations, they verify them with the same tool.

**6c. GitHub Action for everyone**: composite action in the public repo wrapping `npx tokler-cli@<pinned>` (works today on ubuntu runners since linux-x64 is published). Behavior modeled on Infracost: one sticky PR comment (update-in-place), before/after context-weight table for changed agent-context files, repo load-profile totals, tier-labeled savings, optional `fail-on` input. Our existing tokler-audit.yml is the prototype; the action is its generalization. Research found no incumbent token-savings action: open niche.

**6d. Stakeholder artifact**: `tokler report --html` renders a self-contained static HTML report (project or machine scope) for sharing with non-technical stakeholders: load profiles, savings tiers, trend charts, methodology footnotes. No server, one file, print-friendly.

## 4. Pushbacks and decisions (so we do not relitigate)

1. **Do not chase the 65% number.** It is output-side, baseline-dependent, and independently reproduced at 9-21%. Tokler's benchmark leads with measured structural and configuration savings, includes lossy as one labeled track, and wins on methodology. If marketing wants one number, it is Tier 2 realized savings on a real repo, with the formula attached.
2. **"Savings since install" must never conflate identified with realized.** The dashboard always shows the tier. An inflated headline would burn exactly the trust the honesty design bought.
3. **Privacy posture changes are explicit, not silent.** Local ledger and log ingestion are consented at onboarding, documented in CLAUDE.md/AGENTS.md, resettable, and CI-disabled by default. Still zero network egress except the existing opt-in verify.
4. **TUI scope is capped at three tabs for v1.** The heatmap calendar and animations are v1.1 polish. A dashboard that ships beats a dashboard that wows in a branch.
5. **Bare `tokler` opens the dashboard only in a TTY.** Scripts and agents see unchanged behavior; agents get `tokler stats --json`.
6. **The dedup detail matters.** Usage-log ingestion keeps the LAST record per (message.id, requestId); the incumbent's first-wins undercount is a known bug we will not import.

## 5. Phasing

| Phase | Scope | Done when |
|---|---|---|
| I1 | W1 portability (per-OS scan paths, portable bump, windows CI test job) + W2a ledger + W4 onboarding | Fresh user on any OS gets onboarded, scan finds their configs, runs are recorded locally |
| I2 | W2b log ingestion (Claude Code, then Codex) + W2c tier accounting + `tokler stats` (plain + json + global) | Tier 2/3 numbers computable and correct on this machine's real logs |
| I3 | W3 TUI dashboard (3 tabs, compact mode) + Windows publish leg | Bare `tokler` shows the dashboard; `npx tokler-cli` works on Windows |
| I4 | W5 benchmark harness + results page on tokler-web | `RESULTS.md` regenerable by anyone; page live; caveman/LLMLingua comparisons published with methodology |
| I5 | W6 distribution (public repo: skills + plugin + action + report --html) | A stranger can `npx skills add` the tokler skill and a repo can adopt the action in one paste |

Each phase follows the established ritual: parallel agents where parallelizable, full gates, PROGRESS.md entry, version bump, merge to main (auto-publish).

## 6. Open questions for future sessions

1. Ledger schema versioning: JSONL with a `v` field per record vs sidecar schema version. Lean JSONL + `v`.
2. Project identity across moves/renames: canonical path vs git remote hash vs both. Lean both (path primary, remote as alias).
3. Session-rate fallback when logs are not ingested: ask at onboarding ("roughly how many agent sessions/day?") vs default 10/day labeled as assumption.
4. Benchmark quality scoring default: ship BYOK-gated LLM fact-checking or ratios-only at launch. Lean ratios-only with the harness ready.
5. TUI crate sharing: extract render helpers from apps/cli or keep tokler self-contained. Lean self-contained (different lifecycles), copy patterns not code.
6. Whether `tokler report --html` belongs in I3 instead of I5 (stakeholder demand may pull it earlier).

## 7. References

- Original architecture: `apps/tokler-web/PLAN.md` (sections 5 tokenization, 8 audit catalog, 9 MCP, 11 CLI surface, 13 roadmap, 14 risks).
- Work log: `apps/tokler-web/PROGRESS.md` (bottom-up for latest state).
- Context snapshot: `apps/tokler-web/HANDOFF.md`.
- Caveman ecosystem: wilpel/caveman-compression (transformer, 40-58% claimed, thin methodology); JuliusBrussee/caveman (output-side skill, 65% claimed, includes /caveman-stats and caveman-shrink MCP middleware); jwiegley promptdeploy caveman (no numbers); Guzik independent benchmark (Dev.to, 9-21% fair-baseline result, the methodology bar to clear).
- Compression gold standard: microsoft/LLMLingua (up to 20x with ~1.5pt GSM8K drop; published eval scripts).
- Usage logs: ccusage (ryoppippi/ccusage; dedup first-wins undercount bug in issues #866/#888; LiteLLM pricing, blocks view, statusline); Claude Code JSONL format verified firsthand on this machine (usage fields incl. cache splits and per-record cwd); Codex rollout JSONL verified.
- Skills/plugins: vercel-labs/skills (`skills/<name>/SKILL.md`, ~71 agents, universal .agents/skills); Claude Code plugin docs (.claude-plugin/plugin.json, marketplace.json, bin/ on PATH).
- PR comment UX: Infracost actions (sticky update-in-place cost-diff comment); sticky-pull-request-comment building block.
- Ratatui prior art: ccboard, claude-token-monitor, claudelytics, tokenusage; in-repo: apps/cli (ratatui 0.29 + crossterm 0.28 render patterns).
