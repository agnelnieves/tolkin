# Tolkin Local Intelligence Layer: MLX Research Findings

Engagement date: 2026-06-11. Charter: MLX-RESEARCH-PROMPT.md. Repo state at research time: tolkin 0.12.0 (apps/tolkin-cli/Cargo.toml), all EXECUTION-PROMPT-I6 waves merged, public launch complete (PROGRESS.md lines 1023-1025). Real measurements in this report were taken on the research machine: Apple M1 Max (32-core GPU), 64GB unified memory, macOS 26.3, mlx-lm 0.31.3, Qwen3.5-4B-4bit. Everything not measured here is labeled an estimate.

## 1. Executive verdict

1. GO WITH SCOPE CUT, confidence moderate-high. The layer is feasible, defensible, and nobody ships it (competitive scan verified 2026-06-11), but the owner's vision survives only in task-scoped form, not as a monolithic "analyze my whole setup" prompt.
2. The deciding measurement: the blessed 16GB-floor model (Qwen3.5-4B-4bit) prefills at 228-269 tokens per second on an M1 Max through the real mlx-lm stack. The standard analysis bundle defined below is 22,673 tokens. That is 90-100 seconds of time-to-first-token on this machine, and an estimated 4-6 minutes on a base M2. Monolithic deep analysis misses interactive latency on the stated hardware floor.
3. What survives: small, targeted, deterministically gated tasks. Plain-language narration of existing findings (5k-token prompts), skill-file lint (0.5-3k per file), pairwise contradiction checks, and schema-validated draft artifacts. All fit the prefill budget on M2-class machines with honest upfront time estimates.
4. JSON without constrained decoding is workable: 5 of 6 strict-schema runs validated on the first try; the single failure was a max-tokens truncation, which Rust detects deterministically (generation length equals the cap) and repairs with one bounded retry. mlx-lm upstream still has no response_format (verified: PR 845 closed unmerged, issue 1007 open).
5. The identity survives intact: deterministic surfaces stay bit-identical, model output gets a new "model advisory" class that is never a tier, content reading gets a new explicit consent, CI never touches the model path, and every model proposal passes a deterministic gate (re-tokenize, re-measure, schema-validate, span-ground) before presentation.
6. The moat is the ledger: tolkin can measure, in the measured tier, whether applied local-model advice actually saved tokens. No cloud competitor can close that loop without exfiltrating content.
7. v1 ships detection, consent, narration, and skill lint against any OpenAI-compatible loopback server (mlx-lm, Ollama, LM Studio, llama-server), spawns nothing, downloads nothing, and adds zero base-install weight.
8. 8GB machines are out for the full layer, in for narration-only degraded mode on 0.8-2B models. 16GB is the honest floor. M5-generation machines (verified 3.3-4.1x TTFT) eventually make the monolithic mode viable; do not design for them now.
9. The spike that could kill it: one day on a real base M2, three repos, kill criteria defined in section 8. Run it before committing slice 1.
10. One genuinely new option surfaced: Apple's Foundation Models framework (OS-provided on-device model with guided generation) could give the narration tier constrained decoding and zero download on macOS 26+. Filed as an open question to verify in the spike, not a finding.

## 2. Hardware reality and the tier table

### 2.1 Measured on this machine

Configuration: M1 Max, 32-core GPU, 64GB, macOS 26.3, mlx-lm 0.31.3 (PyPI latest as of 2026-06-11), mlx-community/Qwen3.5-4B-4bit (3.03GB weights), prompts built from this repo's real agent-context files, chat template applied, 160 max new tokens, single run per size, lightly loaded desktop. Model load: 2.2 seconds.

| Prompt tokens | Prefill tok/s | TTFT | Decode tok/s | Peak memory |
|---|---|---|---|---|
| 789 | 174.2 | 4.7s | 47.9 | 3.56GB |
| 3,789 | 269.3 | 14.2s | 46.0 | 4.22GB |
| 7,789 | 257.7 | 30.4s | 42.9 | 4.61GB |
| 15,789 | 246.0 | 64.4s | 38.9 | 5.42GB |
| 31,789 | 228.0 | 139.6s | 33.0 | 7.03GB |

Observations. Prefill plateaus near 260 tok/s and degrades gently with context (about 15 percent from 4k to 32k). Decode degrades 31 percent over the same span, consistent with the verified 30-40 percent band for 4-bit models at 32k (github.com/ml-explore/mlx/discussions/3209, which corrected the prior 20 percent claim). Peak memory stays near 7GB at 32k, so a 16GB machine holds the 4B model plus an IDE.

The headline correction to the prior grounding pass: table-derived estimates were optimistic. The llama.cpp pp512 table puts this exact chip at 530 tok/s for a 7B Q4, which naively implies 900+ for a 4B; the real mlx-lm Python stack with a chat template delivers about a quarter of that. Latency budgets must come from end-to-end measurements, not kernel benchmarks.

### 2.2 The standard analysis bundle, defined

The bundle a full semantic pass would consume, measured on this repo with the 0.12.0 binary (tolkin count, default provider):

| Component | Tokens |
|---|---|
| scan --json output | 4,434 |
| project --json output | 4,839 |
| All instruction and skill files (root CLAUDE.md, tolkin-web CLAUDE.md, AGENTS.md, four SKILL.md files) | 13,400 |
| Total | 22,673 |

This repo is heavy (22 context files, 44,174 always-plus-on-invocation context tokens per project totals). Cross-tokenizer variance (tolkin's counts vs Qwen's BPE) is within roughly 10-15 percent; treat bundle sizes as estimates when fed to a Qwen model. Task-scoped prompts are far smaller: narration consumes one JSON document (about 5k), skill lint consumes one file (0.5-3k).

### 2.3 Tier table

Scaling method for non-measured cells: measured M1 Max plateau (about 250 tok/s prefill, 40 tok/s decode mid-context) scaled by the verified llama.cpp 7B Q4_0 pp512 ratios (M1 Max 32c 530.06, base M2 179.57, base M4 221.29, M4 Pro 439.78, M4 Max 885.68; github.com/ggml-org/llama.cpp/discussions/4167). Decode scales roughly with memory bandwidth. Every non-M1-Max number is an estimate and labeled E.

| RAM / chip class | Blessed model (4-bit, verified size and license) | Narration task (about 5k in, 300 out) | Full bundle (22.7k in) | Call |
|---|---|---|---|---|
| 8GB, any M-series | Qwen3.5-0.8B (0.63GB) or 2B (1.72GB), Apache 2.0; Gemma 4 E2B as alternate | E 15-40s | not offered (memory ceiling about 5.3GB shared) | Degrade: narration and single-file lint only |
| 16GB, base M2/M3 | Qwen3.5-4B (3.03GB), Apache 2.0 | E 70-110s | E 4-6 min | Recommend task-scoped; full bundle behind an explicit time estimate and confirm |
| 16GB, base M4 | Qwen3.5-4B | E 55-90s | E 3.5-5 min | Same as above, slightly better |
| 32GB, M2/M3/M4 Pro | Qwen3.5-9B (5.95GB), Apache 2.0 | E 35-60s | E 2-3 min | Recommend |
| 48-64GB, Max-class | Qwen3.5-35B-A3B MoE (20.39GB, about 3B active), Apache 2.0 | E 20-40s at 9B-plus quality | E 1-2 min (measured here: 90-100s prefill on the 4B) | Recommend; note the MoE quant quality flag (mlx-lm issue 1011, degraded tool calling on 4/8-bit 35B-A3B checkpoints) |
| M5 generation, any | revisit | E divide M4 times by 3.3-4.1 (verified TTFT multiplier, machinelearning.apple.com/research/exploring-llms-mlx-m5) | E under 90s on base M5 | Monolithic mode becomes plausible; do not assume in v1 |

Floor decision: 8GB is out for the layer proper and honestly stated as narration-only. 16GB is the floor. The owner's M2-minimum fleet lands in the two 16GB rows unless machines have 32GB, so the v1 task list must be sized to the 16GB M2 row. What a 16GB M2 user waits for, concretely: about a minute and a half for a narrated optimization plan, and 6-35 seconds per skill file linted (E).

gpt-oss-20b (12.08GB MXFP4, Apache 2.0) and Phi-4-mini (2.16GB, MIT) verified available as alternates. Gemma 4 is now Apache 2.0 including E2B/E4B edge variants (verified on google/gemma-4-12B-it and gemma-4-E4B-it model cards), so the owner's "Gemma too heavy" instinct resolves as: Gemma 3 12B/27B remain too heavy and carry the old license, Gemma 4 E2B/E4B are legitimate 8GB-tier candidates. Qwen3.5 stays the default family for license uniformity and the WWDC-demonstrated path (session 232 demos mlx_lm.server with Qwen3.5).

## 3. Task inventory, scored

Axes: competence (small-model ability at the task), marginal value (over the deterministic engine; the Repomix/Aider tree-sitter counterexample binds: structure that can be computed must not be modeled, Tokens.md:420), verifiability (model proposes, Rust verifies), containment (blast radius when wrong). Scores low/med/high. Latency from section 2.

| Task | Competence | Marginal value | Verifiability | Containment | Verdict |
|---|---|---|---|---|---|
| 1. Plain-language narration of deterministic findings (project, scan, cache, audit JSON in; prose plan out) | high (summarizing structured input is the small-model sweet spot) | high for non-technical users; zero new facts claimed | high: number-faithfulness gate (every figure in output must appear in the input JSON; Rust string-checks) | high: advisory prose, no edits | KEEP, v1 flagship |
| 2. Skill-file lint beyond the static budget rule (trigger specificity, body verbosity, example bloat; spec at Tokens.md:109-135) | med-high with a rubric prompt | high: trigger vagueness is unreachable by regex | med-high: suggested rewrites re-tokenized for measured delta; schema-validated findings; span-grounded quotes | high: per-file, diff-presented | KEEP, v1 |
| 3. Static-vs-volatile span classification feeding the I7-1 cacheability score (REVIEW-FINDINGS.md:218, confirmed uncommissioned) | med (obvious volatiles are regex work; the model adds paragraph-level judgment) | med: deterministic heuristics take the bulk; model is a refiner | high, the best of all: spans are byte ranges consumed by a deterministic score; a wrong span shifts a number, never fabricates text | high | KEEP, v2, only after I7-1 ships its deterministic core first |
| 4. Cross-file paraphrase dedup (the Model2Vec slot, PLAN.md section 8 experimental row) | high for embeddings; generative judge only adjudicates candidate pairs | high: MinHash misses paraphrase | med-high: cosine threshold plus judge verdict, both spans quoted and verified to exist verbatim | high | KEEP, v2; embeddings first, judge second; spike compares them |
| 5. Instruction-file contradiction audit (CLAUDE.md vs AGENTS.md vs rules files) | med (pairwise judgments on small excerpts) | med-high; stale-path detection is deterministic fs work and must NOT use the model | med: span-grounding check (both quoted spans must exist verbatim in source) | high | KEEP, v2, contradictions only |
| 6. SDLC-aware draft skill generation (deterministic manifest parse picks WHAT, model drafts) | med (4B drafts are serviceable, 9B better; per-tier ladder) | high: the only generative artifact in the set; the owner's concrete ask | high: the check-skill-schemas.ts pattern already validates skills against live JSON (apps/tolkin-cli/scripts/check-skill-schemas.ts); drafts re-tokenized against budget; presented as new-file diffs | high: writes only to a proposals dir, never config in place | KEEP, v3 |
| 7. Map-reduce repo summarization for the docs profile | med | low: collides with the repo-packing rejection (REVIEW-FINDINGS.md:163) and the tree-sitter counterexample; latency is the worst (65k tokens measured for this repo's docs corpus) | low | med | CUT; revisit only if the M5 fleet arrives |
| 8. Transcript-content analysis of any kind | n/a | n/a | n/a | n/a | CUT for v1 by posture: usage ingestion stores counts only (apps/tolkin-cli/src/usage/types.rs), content is off-limits (cost_per_successful_task rejection, REVIEW-FINDINGS.md:151); a transcript consent class is a separate future adjudication |

JSON contract for every kept task: prompt discipline (schema in prompt, "ONLY a JSON object"), explicit max_tokens always set (upstream default 512 is a trap, verified in server.py argparse), Rust-side serde validation, truncation detected by generation length equal to the cap or a non-stop finish reason, one bounded retry with a tighter ask, then graceful decline. Measured basis (anecdote, n=6, this machine): 5 valid on first attempt, 1 truncation, 0 malformed-syntax failures. For backends with enforced schemas (llama-server GBNF/json_schema verified; LM Studio structured outputs verified; Ollama MLX path currently broken, issue 16563), tolkin sends response_format and still validates locally; enforcement is a bonus, never assumed.

## 4. Integration architecture

### 4.1 v1 shape: detect, never spawn, never download

The CLI gains one opt-in surface that talks to an existing OpenAI-compatible server on loopback. Detection probes, in order: 127.0.0.1:8080/v1/models (mlx-lm or llama-server), 127.0.0.1:11434 (Ollama), 127.0.0.1:1234 (LM Studio), all verified ports. ureq 2 with the json feature is already the CLI's only HTTP dependency (apps/tolkin-cli/Cargo.toml) and verify.rs is the template for the call pattern; there is no localhost code anywhere in the CLI today (grep confirmed), so this is a clean addition. Zero new base-install weight, no Python in the process tree, no node-gyp, Rust only. This follows the Xcode 26 minimal-responsibility precedent (Settings, Intelligence, Locally Hosted: the host app manages no downloads). Spawning a supervised mlx_lm.server child and managing model downloads with RAM-fit badges is v2 polish at the earliest; the documented v1 setup is one line per backend, for example: uv tool install mlx-lm, then mlx_lm.server --model mlx-community/Qwen3.5-4B-4bit.

Loopback enforcement: the configured base_url must resolve to 127.0.0.1, ::1, or localhost; anything else refuses with an explanation unless TOLKIN_SIDECAR_ALLOW_REMOTE=1 is set explicitly, and even then the command prints a one-line egress warning. This keeps the PRIVACY.md zero-egress statement (distribution/PRIVACY.md lines 6-20) literally true by default: model traffic never leaves the machine.

Server-death and offline behavior: every sidecar call carries a short timeout; on failure the command completes its deterministic skeleton, marks the advisory sections "local model unavailable", and exits 0. A kill -9 mid-run must produce the identical deterministic output (golden test).

### 4.2 Where it lives in the CLI

New subcommand: tolkin optimize. The Commands enum and dispatch live at apps/tolkin-cli/src/cli.rs:29-59 and 62-83; the new arm follows the same Args-struct-plus-run pattern as the existing 15 subcommands. Behavior:

- Without a sidecar (absent, declined, or dead): prints the deterministic optimization plan that project, audit, and mcp already compute, plus one line stating that deeper local analysis is available and how to set it up. Useful on its own, fully deterministic.
- With a sidecar and consent: adds the model-advisory sections (narration, skill lint), each labeled, each gated.
- TTY heuristics follow the existing rule (bare tolkin opens the dashboard only in a TTY, IMPROVEMENTS.md 4.5): interactive wizard in a TTY, plain output when piped, --json always machine-shaped.
- Flags: --task narrate|skills|all, --harness <name> when several harnesses are configured, --json, --model and --base-url overrides, --yes for non-interactive consent suppression (which simply skips model sections, never auto-consents).

Existing commands (count, audit, scan, project, cache, stats, mcp, report, all 15) do not change by one byte. The separation contract in section 6 makes that testable.

The closed loop with skills: distribution/skills/tolkin-optimize already instructs a cloud agent to run the deterministic commands and propose changes. With the layer present, that skill's instructions gain one paragraph: prefer tolkin optimize --json local output as the proposal source, which moves the semantic work off the paid harness tokens onto the local sidecar. Tolkin's own skills get cheaper to run; that is the nice closed loop, and it requires no new skill, only a SKILL.md edit validated by the existing check-skill-schemas.ts drift lint.

### 4.3 Config and consent

Config: a [sidecar] table in the existing config.toml (created by ledger::Config at apps/tolkin-cli/src/ledger.rs:100-123, version field already present, platform data dir via the directories crate): enabled, base_url, model, max_context, plus nothing else in v1. Forward-compatible Option fields, exactly like session_rate_per_day already demonstrates.

Consent: a new boolean consent_local_model alongside consent_ledger and consent_log_ingestion (ledger.rs:102-103), asked the first time tolkin optimize detects a sidecar, never at install. The consent text states, concretely: which file classes the model will read (instruction files, skill files, MCP config files, the same set scan already reads; never shell configs, never secrets, never transcripts), that traffic stays on loopback, that output is advisory and never auto-applied, and how to revoke (re-run tolkin init, or set enabled=false). This satisfies the IMPROVEMENTS.md 4.3 bar verbatim: explicit, documented in CLAUDE.md and AGENTS.md and PRIVACY.md, resettable, and CI-disabled (the model path refuses when CI=true or TOLKIN_NO_LEDGER-style TOLKIN_NO_SIDECAR=1 is set, mirroring ledger gating at ledger.rs:68-72).

Redaction order: file content passes through the existing always-on redactor before it reaches the prompt builder, same doctrine as every other surface (redact runs before anything else, apps/tolkin-cli/src/cli.rs command list). The model never sees a secret value even though the traffic is loopback.

### 4.4 The Python question

Documented uv or pipx installation of mlx-lm is acceptable for v1; the user runs their own sidecar exactly as Xcode expects, and tolkin's posture (no Python in the base install, ever) is untouched because tolkin only speaks HTTP. Ollama is the best download UX for non-technical users today (one app, model pulls built in) but its MLX backend is a preview gated to Macs above 32GB (verified, ollama.com/blog/mlx) and its structured output is broken on that path, so the docs bless: mlx-lm via uv for the technical path, LM Studio for the GUI path (free for work since 2025-07-08, headless daemon available), Ollama and llama-server as the cross-platform story. An embedded Swift helper on mlx-swift-lm remains a later polish option; the xcodebuild-only Metal shader build gotcha makes it a packaging project of its own, not v1.

### 4.5 Concurrency

mlx_lm.server ships continuous batching with prompt-concurrency 8 and decode-concurrency 32 defaults and a prompt cache of up to 10 KV caches (verified in server.py argparse). Default parallelism for per-file tasks: 4 in-flight requests, sized well under the prompt-concurrency default. The static rubric prefix of each task is identical across files by construction (rubric first, file last), so the server's prompt cache absorbs the rubric prefill once and each subsequent lint pays only its file; that ordering is the same static-prefix discipline tolkin preaches for cloud caches (Tokens.md:74-89) applied to itself.

### 4.6 Windows and Linux, one paragraph

The same layer works unchanged against Ollama or llama-server on 11434/8080, both verified OpenAI-compatible with structured output (llama-server with full GBNF/json_schema enforcement, the strongest of all backends). The decision: supported but unblessed in v1. Docs state the probe works, name the two backends, and make no latency promises because no measurements exist; the blessed fast path and all UX tuning target Apple silicon plus MLX first. Revisit after the spike if Linux developer demand shows up.

## 5. Experience design

### 5.1 No identity question

The pre-flight "which kind of user are you" question is rejected on its merits: it adds a decision point before value, it ages badly (users change modes per task, not per identity), and the invocation already carries the signal. Inference rules: TTY plus zero flags equals wizard; any of --json, --task, --fail-on equals expert; piped stdout equals expert. This matches the precedent set by bare tolkin's TTY-only dashboard (IMPROVEMENTS.md 4.5) and by Xcode, Zed, and Continue, none of which ask.

### 5.2 The non-technical golden path

1. tolkin optimize in a TTY. Sidecar detected, first run: consent screen (section 4.3), plain words, one y/n.
2. Upfront time estimate, mandatory given section 2 latencies: tolkin already embeds tokenizers (tiktoken-rs, the 17.5MB Gemma tokenizer at apps/tolkin-cli/src/tokenize/mod.rs:120), so it counts every input before any model call and prints, for example: "Narration about 70s, 4 skill files about 90s, total about 3 minutes on this Mac (M2, 16GB, Qwen3.5-4B)". The per-chip rates ship as a small static table seeded from this report's measurements and the spike, labeled estimates.
3. Streaming progress per task with partial results as each completes; Ctrl-C keeps whatever finished.
4. Artifacts: an OPTIMIZATION-PLAN.md in plain language (every number traceable to the deterministic JSON, gate enforced), plus proposed diffs in a proposals directory. Nothing applied. The wizard offers per-file y/n apply with a diff shown, the same confirm-before-apply doctrine the skills already follow.
5. Closing line points at the loop: "After applying, re-run tolkin project and tolkin stats; the ledger records the measured delta." That delta lands in the measured tier because the deterministic engine measures it; the advisory itself never does.

### 5.3 The technical path

tolkin optimize --task skills --json | jq, composable, stable exit codes (0 success including model-unavailable, 2 reserved for the existing --fail-on semantics which never consult the model), per-task flags, --harness to disambiguate when scan finds several configured harnesses, and everything the wizard does available non-interactively except consent itself.

### 5.4 Harness divergence

The layer consumes scan's per-harness inventory rather than re-deriving it. Catalog currency verified: MCP_CATALOG_OBSERVED is "2026-06" (packages/tolkin-core/crates/core/src/mcp.rs:154), the client catalog spans Claude Desktop and Claude Code (correctly distinguished), Cursor, Codex, VS Code, Zed, Continue, Windsurf, Gemini CLI, plus A12's .windsurf/rules and .clinerules and the workflow LLM detector (apps/tolkin-cli/src/scan/mod.rs:132-289 and 881-1169). Verified gaps to file as scan work, not layer work: OpenCode, JetBrains (Junie), Amp, Aider. When several harnesses are present the wizard asks once; the expert path takes --harness; the default optimizes the harness whose config the cwd most specifically matches (project-level beats global), stated in the plan header.

## 6. Identity adjudications

1. Separation contract. The deterministic core keeps zero LLM calls: the sidecar module is only reachable from the optimize command; it may read only files scan and project already discover (post-redaction) and their --json outputs; it may emit only model-advisory blocks and files under the proposals directory; no deterministic output, exit code, or CI gate ever consults it. Enforcement is a golden-output test running every existing command with the sidecar absent, present, and killed mid-run, asserting byte identity, plus the PRIVACY.md-style cross-reference (the grep ritual PROGRESS.md line 334 records for the web analyzer applies here: one sanctioned module, everything else clean).
2. Consent class. Content-reading local analysis is a new consent even with zero egress; adjudicated IN, designed in section 4.3 to the IMPROVEMENTS.md 4.3 bar (explicit, documented, resettable, CI-disabled). Transcript content stays OUT entirely; the volatile-span task therefore operates only on context files in v2, and any transcript variant is a separate future engagement. What this costs: the cache layer's session-specific judgment stays heuristic, an accepted loss.
3. Advisory label. Model output is a fourth display class, "model advisory (local)", with the tier system untouched (LABEL_IDENTIFIED, LABEL_REALIZED, LABEL_MEASURED at apps/tolkin-cli/src/tiers.rs:79-83). In JSON: {"class": "model_advisory", "model": "<id>", ...} in a dedicated array, never summed into tier totals, never rendered without the suffix. When a user applies an advisory and re-measures, the delta enters the measured tier through the deterministic engine, which is the only door.
4. CI. The model path is hard-off in CI, full stop. Challenged and upheld: even temp-0 decoding drifts across model files, quant revisions, and mlx versions; a gate that can flake with a brew upgrade is not a gate. The action and --fail-on remain deterministic-only.
5. Recorded rejections, surfaced not averaged. PLAN.md:42 ("WebLLM-style local inference. Right tool for 'rewrite my prompt'; wrong tool for 'audit it.'") was written about the web analyzer surface; this design honors its substance by keeping inference out of every audit path and inside a new opt-in surface, the same pattern the shipped LLMLingua-2 preview already established with disclosed download and explicit opt-in. REVIEW-FINDINGS.md:178 ("do not put LLM calls in the analyzer") is satisfied literally: the analyzer makes no LLM calls; optimize is not the analyzer and degrades to deterministic output. The Layer-3 autonomous-rewrite rejection (REVIEW-FINDINGS.md:152) is honored by diff-plus-confirm everywhere. The runtime-proxy and repo-packing rejections are untouched (task 7 was cut partly on the latter). If the owner reads PLAN.md:42 as banning all local inference in any surface, this layer dies there; that reading should be made explicitly, not by silence.
6. Self-verification roadmap. Because the ledger already reconciles token-for-token against independent recompute (169 sessions at I2; 0.97522 measured hit rate in wave 1), slice 3 adds an applied-advisory marker so stats can report "advisories applied N, measured delta X tokens" with tier-correct labels. That sentence is the product's answer to "did the model actually help", and it is unique in the niche.

## 7. Risks, ranked

1. Base-M2 latency disappointment. Highest risk. Measured here, estimated there: a 16GB M2 user waits about 90s for narration. Mitigations: task-scoped prompts only, mandatory upfront estimates from real token counts, model ladder per tier, server prompt-cache exploitation. Kill criterion in the spike.
2. JSON reliability without constrained decoding. Measured failure mode is truncation, which is deterministically detectable; mitigation is explicit max_tokens, finish-reason checks, one bounded retry, llama-server or LM Studio for enforced schemas. Residual risk low for v1's two tasks.
3. mlx-lm and model churn. Monthly minors; PR 845 (structured outputs) closed unmerged once already. Mitigation: tolkin depends only on the OpenAI-compatible surface, ships a tested-backends matrix with versions, and degrades to detect-only on probe mismatch.
4. Download and support burden. 0.63-20GB per model. Mitigation: v1 manages no downloads (detect-only), the docs own the one-liner installs, the uninstall section names the exact paths including ~/.cache/huggingface/hub.
5. Consent-surface creep. A botched content-consent UX taints the zero-egress story that PRIVACY.md stakes the brand on. Mitigation: section 4.3's design, PRIVACY.md update shipping in the same PR as the feature, scan-the-source ritual extended to the sidecar module.
6. License drift. Qwen3.5 and Gemma 4 Apache 2.0 verified today on the named repos; the 4B MLX conversion repo itself is missing its license tag (an upload gap). Mitigation: the blessed-model table pins license and verify date per entry; cargo-deny culture extended to the model table.
7. MoE quant quality on the 48GB-plus tier (mlx-lm issue 1011). Mitigation: bless dense 9B for 32GB, flag the MoE row until the issue closes.
8. Fast-follow. The lane is verified open (no shipping product does local-LLM config optimization; Repomix remains deterministic-no-LLM by design). The moat is sections 6.6 and 3's gates, which require tolkin's measured tier to copy; speed matters less than shipping the loop.

## 8. Roadmap

Slice 1, S-M, the smallest honest slice. Context: sections 2-6; files: new apps/tolkin-cli/src/sidecar.rs (probe, loopback enforcement, OpenAI-compatible client on ureq), new src/commands/optimize.rs (deterministic skeleton, consent flow, narration task, skill-lint task), consent field in ledger.rs Config, [sidecar] in config.toml, PRIVACY.md and CLAUDE.md and AGENTS.md updates, golden bit-identity tests, ETA table seeded from this report. Contract: deterministic surfaces unchanged byte-for-byte; model output only under model_advisory; narration passes the number-faithfulness gate (every figure traceable to input JSON, enforced in Rust); lint suggestions ship with re-tokenized deltas; works against mlx-lm, LM Studio, Ollama, llama-server probes; declines gracefully when absent, dead, or unconsented (exit 0, deterministic plan intact). Acceptance: a 16GB M2-class machine completes narration inside the printed estimate; kill -9 of the server mid-run leaves output deterministic; check-skill-schemas.ts extended to the optimize --json schema; the measured story is "narration of this repo's real project JSON, numbers verified by gate, on named hardware".

Slice 2, M. User: repo owners on 16-32GB machines. Adds: contradiction audit (span-grounded), paraphrase dedup behind an embeddings prefilter (the Model2Vec slot finally lands, evaluated against a generative judge in the spike), per-file parallelism tuned to the server prompt cache, --harness UX. Honest metric: duplicate-token findings carry exact byte spans and re-tokenized counts; advisory precision sampled by hand on three real repos and reported in the PR.

Slice 3, M-L. User: the same, plus tolkin's own skills. Adds: draft skill generation gated by the schema lint, the tolkin-optimize skill preferring the local layer, and the applied-advisory measured-delta loop in stats. Honest metric: stats prints "N advisories applied, measured delta X input tokens over Y sessions" from the ledger, tier-labeled, on the researcher's machine first.

The spike before slice 1, one day, no product code. On a real base M2 (16GB) and ideally one 8GB machine: run the narration and skill-lint harness as scripts against three real repos with Qwen3.5-0.8B, 2B, and 4B; measure wall time, number-faithfulness rate, lint acceptance rate by hand. Kill criteria: narration faithfulness under 95 percent after one repair retry, or narration P50 over 120 seconds on the 16GB M2 with the 4B, or hand-judged lint usefulness under half. Also in the spike: verify the Apple Foundation Models option (section 9), and the llama-server json_schema path as the enforced-output alternative. Any kill criterion firing drops v1 to the embeddings-plus-narration-on-0.8B subset or to nothing, and the findings get a one-page addendum either way.

## 9. What neither the owner nor the prompt considered

1. Apple Foundation Models framework. macOS 26 ships an OS-provided on-device model (about 3B class) with Swift guided generation, which is constrained decoding with schema guarantees, zero download, zero Python. A small Swift helper speaking JSON over stdio could serve the narration tier on any Apple Intelligence Mac. Open question to verify in the spike: model availability on M1, quality at narration, and helper packaging cost. If it pans out, the 8GB row improves and the JSON risk shrinks for the smallest tier.
2. The server prompt cache as product UX. mlx-lm holds up to 10 KV caches; ordering every task prompt rubric-first makes the rubric prefill a one-time cost per session. The per-file lint loop then pays only file tokens. This is tolkin's own static-prefix doctrine applied to itself and it materially changes the per-file latency story; no extra code beyond prompt ordering.
3. Truncation is the JSON failure mode, not syntax. The measured run's only failure was max_tokens exhaustion mid-string, and upstream's default cap is 512. Always set max_tokens explicitly and check the finish reason; that one rule converts the main reliability risk into a detectable, retryable event.
4. Tolkin already owns the pre-call tokenizer. The embedded tokenizers make honest upfront ETAs possible (count first, then promise), which no generic sidecar wrapper can do as precisely. It is the difference between a progress bar and a guess.
5. Thermals and battery. A multi-minute prefill on a fanless Air is a thermal event and a battery line item; the wizard should say "plugged in recommended" above the confirm on full-bundle runs. Not modeled further here; estimate-class concern.
6. Distributed inference (WWDC26 session 233, up to 3x over Thunderbolt RDMA across Macs) could one day serve an office fleet a shared loopback-adjacent sidecar, but it breaches the loopback promise and is explicitly out of scope; noted so nobody mistakes it for a near-term option.

## Appendix

### A. Corrections to the research prompt's snapshot

- Version: prompt said 0.10.0 moving fast; repo is 0.12.0, waves 0-3 all merged, public launch executed 2026-06-11 (PROGRESS.md lines 973, 1009, 1023-1025). A11-A14 shipped: do-not-re-propose list honored (Homebrew tap live, .windsurf/rules and .clinerules and workflow detector in scan, tolkin-cache skill, PRIVACY.md, bench verification expansion).
- Qwen3.5-4B-4bit is 3.03GB, not about 2.3GB (HF API, verified twice).
- Long-context degradation at 4-bit is 30-40 percent by 32k, not about 20 percent (mlx discussion 3209 covers quant-dependence; decode measured here degraded 31 percent).
- The 22-server MCP catalog stands at 22 entries (counted in mcp.rs CATALOG), stamped 2026-06 (mcp.rs:154).
- One sub-agent during this engagement reported I7-1 as commissioned; verified false: I7-1 appears only in REVIEW-FINDINGS.md:218's candidate table and in no execution prompt or progress entry. The research prompt was right.
- Stale-version housekeeping observed in passing (not findings of this engagement): plugin.json and marketplace.json and action defaults at 0.9.0, formula template at 0.11.0, benchmarks RESULTS.md stamped 0.11.0.

### B. Real measurements, hardware disclosed

Machine: MacBook Pro, Apple M1 Max, 32-core GPU, 64GB unified memory, macOS 26.3 (build 25D5112c), lightly loaded desktop, single run per cell, mlx-lm 0.31.3 via a throwaway venv on Python 3.14.5, model mlx-community/Qwen3.5-4B-4bit. Note this chip is above the fleet floor; all M2/M4 numbers in section 2.3 are estimates scaled by the cited llama.cpp pp512 ratios. The prefill and JSON tables in sections 2.1 and 3 are verbatim from /tmp/mlx-bench-results.json produced by the harness script. JSON reliability: 6 runs, schema-bound prompt at about 3.8k tokens, 1 run temp 0.0 and 5 runs temp 0.7, results 5 valid (all raw JSON, no fences), 1 truncation at the 400-token cap, 0 syntax failures. Anecdote-grade (n=6), not a benchmark.

Tolkin measurements: binary built read-only from this repo at 0.12.0 (cargo build --release), TOLKIN_DATA_DIR pointed at a temp dir, TOLKIN_NO_LEDGER=1. tolkin count outputs: instruction corpus 13,400 tokens, docs corpus 65,026 tokens, project --json 4,839 tokens, scan --json 4,434 tokens; project totals for this repo: 593 files scanned, 22 context files, 44,174 context tokens.

### C. Cleanup

Experiment artifacts removed after this report was written: the throwaway venv at /tmp/tolkin-mlx-venv, the HF cache entry models--mlx-community--Qwen3.5-4B-4bit (3.03GB), the corpus and result files in /tmp. Bytes freed are recorded in the engagement log in chat. Rebuild cost if a re-measure is wanted: about 3 minutes plus a 3GB download.

### D. Files read (repo)

apps/tolkin-web/MLX-RESEARCH-PROMPT.md; PROGRESS.md (bottom-up through line 1031); EXECUTION-PROMPT-I6.md; PLAN.md sections 1, 8, 9-11; IMPROVEMENTS.md section 4; REVIEW-FINDINGS.md (matrix rows 146-180, caching deep dive, I7 table at 215-222, gaps 186-193); LESSONS.md; CLAUDE.md (root and tolkin-web) and AGENTS.md; distribution/PRIVACY.md, README.md, homebrew/Formula/tolkin.rb, .claude-plugin manifests, skills (all four SKILL.md); apps/tolkin-cli/src/{cli.rs, scan/mod.rs, commands/project.rs, commands/cache.rs, cache_analysis.rs, tiers.rs, usage/types.rs, ledger.rs, onboard.rs, verify.rs, tokenize/mod.rs}; apps/tolkin-cli/scripts/check-skill-schemas.ts; packages/tolkin-core/crates/core/src/{audit.rs, mcp.rs}; apps/tolkin-cli/Cargo.toml; /Users/agnel/Downloads/Tokens.md (487 lines, second research begins line 371; load-bearing extracts at lines 68-91, 109-135, 230-252, 353, 385-392, 416, 420, 452-454).

### E. Commands run

sw_vers, sysctl, system_profiler (hardware disclosure); cargo build --release (read-only build); tolkin --version, count, project --json, scan --json under temp data dir; python3 venv creation, pip install mlx-lm; hf download mlx-community/Qwen3.5-4B-4bit; the benchmark harness (prefill series and JSON series); HF API queries for model sizes; greps verifying PLAN.md:42, REVIEW-FINDINGS.md:178 and 218, tiers.rs labels, mcp.rs:154, ledger.rs Config, absence of localhost references; cleanup removals.

### F. URLs consulted (all verified 2026-06-11 unless noted)

github.com/ml-explore/mlx-lm/releases (v0.31.3, 2026-04-22, still latest); raw server.py on main (no response_format; defaults: port 8080, prompt-concurrency 8, decode-concurrency 32, prompt-cache-size 10, max-tokens 512, prefill-step-size 2048); github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md (production warning); issues 1007 and 852 open, PR 845 closed unmerged 2026-02-08; issue 1011 (MoE quant quality). huggingface.co/mlx-community/Qwen3.5-4B-4bit (3.03GB), Qwen3.5-9B-4bit (5.95GB, apache-2.0), Qwen3.5-35B-A3B-4bit (20.39GB, apache-2.0), Qwen3-Coder-30B-A3B-Instruct-4bit (17.18GB), gpt-oss-20b-MXFP4-Q8 (12.08GB), Phi-4-mini-instruct-4bit (2.16GB, MIT); google/gemma-4-12B-it and gemma-4-E4B-it (apache-2.0, ungated; license page ai.google.dev/gemma/docs/gemma_4_license); google/gemma-3-4b-it (still gemma license, gated). github.com/ggml-org/llama.cpp/discussions/4167 (pp512 7B Q4_0: M1 Max 400.26/530.06, M2 179.57, M2 Max 537.60/671.31, M4 221.29, M4 Pro 364.06/439.78, M4 Max 713.93/885.68; M5 rows placeholder). github.com/ml-explore/mlx/discussions/3209 (decode degradation by quant, 2026-03-05 update). machinelearning.apple.com/research/exploring-llms-mlx-m5 (TTFT 3.33-4.06x vs M4, decode 1.19-1.27x). developer.apple.com/videos/play/wwdc2026/232 and /233 (titles and claims verified; 232 demos mlx_lm.server with Qwen3.5). ollama.com/blog/mlx (2026-03-30, preview, above-32GB gate); github.com/ollama/ollama issue 16563 (structured outputs ignored on MLX path, updated 2026-06-10). lmstudio.ai/blog/free-for-work (2025-07-08); lmstudio.ai/docs/developer (port 1234, structured outputs, headless; docs-level). github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md (grammar, json_schema, response_format). microsoft.com/en-us/research/project/llmlingua (research line, not a product); github.com/yamadashy/repomix (deterministic, no LLM in pipeline). Competitive scan negative result after three search passes: no shipping local-LLM config or prompt optimizer found.
