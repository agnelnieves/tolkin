# Tolkin I6/I7 Execution Prompt: Review Remediation, Cache Layer, CI Deltas, Homebrew

Copy everything below the line into a fresh agent session (Claude Fable 5, /effort max). It is self-contained: the context, the binding rules, the per-agent task specs with model assignments, and the researched Homebrew distribution spec. The orchestrator session you are starting dispatches sub-agents; it does not do all the work itself.

---

You are the ORCHESTRATOR for the next Tolkin engagement. A cross-reference review (two independent researches versus everything built) completed on 2026-06-10 and produced a prioritized findings report. Your mission: execute that report's recommendations as a sequence of waves, dispatching sub-agents with the model assignments specified below, shipping each wave through the established gate-and-publish ritual.

Authority level, granted explicitly: autonomous merge-on-green. Implement, verify, merge to main (auto-publish via trusted publishing), and keep going. Stop ONLY for owner-account actions, which are consolidated in section 7. Never commit secrets, never weaken a gate to pass it.

## 1. Required reading, in order, before any work

1. `apps/tolkin-web/REVIEW-FINDINGS.md`. This is the work order. Read it end to end: the P0/P1 register with file and line citations, the prompt-caching deep dive (your Wave 1 spec), the cross-reference matrix, and the I6/I7 table.
2. `apps/tolkin-web/IMPROVEMENTS.md` section 4 (decisions never to relitigate: no runtime proxy, no output-policy product, no chasing headline percentages, tiers never conflated, bare `tolkin` TTY-only).
3. `apps/tolkin-web/PLAN.md` sections 8 (audit catalog), 9 (MCP analyzer), 10 (cost), 11 (CLI surface).
4. `apps/tolkin-web/PROGRESS.md` bottom-up (the I2 reconciliation ritual and the rename entry matter most).
5. Root `CLAUDE.md` and `apps/tolkin-web/CLAUDE.md` (binding conventions).

If `REVIEW-FINDINGS.md` or this prompt file are untracked when you start, commit them first as a docs commit (no AI attribution lines, ever).

## 2. Binding rules (recap; violations are ship blockers)

- No em-dashes or en-dashes in ANY content you or your agents write: code comments, docs, commit messages, formula files, PROGRESS entries. Hyphens in compounds are fine. Scan before every commit.
- bun only (never npm/pnpm/yarn for installs); Rust-first toolchain; Biome and oxlint; tsgo; Turbopack.
- Tier vocabulary used exactly: identified (advisory estimate), realized (measured structure, estimated frequency), measured (ground truth). Every savings number carries its label. All claims input-token bounded.
- Privacy posture: zero egress of user content; ledger and ingestion consented, CI-disabled, resettable. The only sanctioned product fetch is the opt-in BYOK verify.
- Never link the private monorepo from anything public-facing (distribution/, formulas, READMEs).
- Update PROGRESS.md at the end of every work unit (the review session was forbidden from doing this; you are required to).
- Version bumps via `bun run --filter=tolkin-cli bump`; merges to main auto-publish all six npm packages via OIDC trusted publishing.

Gate suite per wave (all must pass on the combined tree, run by YOU, not trusted from agent summaries): cargo test, clippy -D warnings, fmt --check, cargo-deny on both Rust workspaces; wasm-pack release build; the four web gates (lint, lint:fast, typecheck, build); dash scan; egress scan (`grep -rn "fetch(" apps/tolkin-web/src | grep -v "src/lib/verify"` returns zero); `bun pm untrusted` returns 0; bench determinism check whenever benchmarks/ or core counting paths change; browser verification for any web UI change.

## 3. Sub-agent model policy (assign exactly as specified per task)

- **Inherit (omit the model parameter; the agent runs Fable 5 with the 1M context window):** reserved for wide, cross-cutting feature work that must hold many files, docs, and money-math in context simultaneously with maximum accuracy. Used for A6 (cache layer) and A9 (tools/list plus bench upgrade).
- **opus:** deep, bounded implementation requiring strong reasoning in a normal context window, and EVERY adversarial review pass. A reviewer is always a different agent from the author.
- **sonnet:** well-scoped implementation, docs, workflows, schema regeneration, formula authoring. Fast and accurate enough when the gates backstop it.
- **haiku:** read-only sweeps only (schema extraction from live output, link checks, dash scans at scale, inventory greps). Never code edits.
- Research agents that need the web get sonnet with explicit instructions to cite URLs.
- Parallelism rules: parallelize only agents whose file sets do not overlap; use worktree isolation for parallel code agents; you re-run the full gate suite on the merged tree afterward. Trust but verify: read what landed before believing any summary (LESSONS.md has the receipts).

## 4. Waves and per-agent specs

Ship each wave as its own version (suggested: Wave 0 as 0.9.1, Wave 1 as 0.10.0, Wave 2 as 0.11.0, Wave 3 as 0.12.0; use judgment if reality disagrees). Within a wave, agents marked parallel-safe may run concurrently.

### Wave 0: hotfixes (the P0s and P1s; small, surgical, no scope creep)

**A1 (sonnet): un-break the PR audit workflow.** `.github/workflows/tolkin-audit.yml` has two top-level `env:` keys (lines 14 and 25; introduced by commit 168fe98). GitHub rejects the file; every push since shows a 0-second failure. Merge into ONE `env:` mapping carrying both `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: "true"` and `TOLKIN_FAIL_ON: ""` with their comments. Acceptance: duplicate-key grep (`grep -c "^env:"` equals 1); after merge, `gh run list --workflow=tolkin-audit.yml` shows no new 0-second parse failures; open a trivial docs PR against main to prove the sticky comment posts, then merge or close it.

**A2 (opus): make cache discounts self-consistent.** In `packages/tolkin-core/crates/core/src/mcp.rs`, `scenarios()` (line ~752) hardcodes warm multipliers (OpenAI 0.50, Gemini 0.25) that contradict `pricing.rs` (OpenAI cache_read is 10 percent of input across the GPT-5 family; Gemini has no modeled discount). Derive the warm multiplier from `pricing::default_for(provider)` (cache_read over input, falling back to 1.0 when None) instead of hardcoding. Fix the same-report contradiction P1-5: the GitHub catalog note says Tool Search drops it to "about 8.7K" while the computed tool_search scenario is 3,500; align them (either scale the stub estimate from the server's cold cost or correct the note to reference the computed scenario; pick one and say why in PROGRESS). Update the affected tests (the vscode warm 0.50 assertion will change) and PLAN.md section 9's scenario table so the source of truth matches the code. Gemini stance: wait for A-R1's memo; if current Google docs publish a cached-token discount for the 2.5 family, add `cache_read` to the Gemini rows in pricing.rs and bump `PRICES_OBSERVED`; if not verifiable, keep None and soften the cost.rs note from "No published cache-read discount for this model" to "No cache-read discount modeled in this table". Acceptance: no hardcoded warm multipliers remain; `tolkin mcp --provider openai` warm equals the pricing-table ratio; all gates green.

**A3 (sonnet): regenerate the skills from live contracts, and make drift impossible.** The staged skills document things the product does not emit. Fix in `distribution/skills/`: (a) tolkin-audit step 3 names rules `oversized-skill-body` and `shell-export-secret` which exist nowhere; replace with real rule ids from a live `tolkin project . --json` run (the 13 real ids are listed in REVIEW-FINDINGS) and route secrets guidance at the real `secret_files` field; (b) tolkin-optimize step 1 documents a `stats --json` schema whose keys do not exist; regenerate that block from live output (top-level: scope, project_key, generated_at, prices_observed, realized_rate, ledger, ingestion, tiers); (c) fix the "across files" near-duplicate overclaim (the audit runs per file); (d) bump all three `metadata.version` fields to the shipping version. Then add a schema-drift lint: a small bun script that extracts every JSON key documented in the three SKILL.md files and asserts each appears in live `--json` output from the built binary; wire it into tolkin-ci (or the action dry-run workflow). Acceptance: the lint passes and would have caught all three drifts (prove by reverting one doc line and watching it fail, then restore).

**A4 (sonnet): distribution and benchmark truth fixes.** (a) `distribution/README.md`: Windows row says "pending first publish" but tolkin-win32-x64@0.9.0 is live; the quickstart captions `scan` as "Count tokens in a file"; the closing line points at `benchmarks/RESULTS.md` which the staged repo does not contain. Fix all three; for the benchmarks pointer, mirror `RESULTS.md` into `distribution/benchmarks/` (add a sync step to the bench runner so the mirror cannot go stale) or link the /bench URL; prefer the mirror. (b) `distribution/action/action.yml` pins setup-node to Node 20 (past end-of-life); bump to 24. (c) Interim honesty fix for the benchmark configuration track (P1-3): `apps/tolkin-cli/benchmarks/methodology.md` track 2 claims counts "against a catalog of real, public server manifests" while the shipped numbers are catalog estimates; reword to state the catalog-estimate basis plainly, adjust the /bench page description (it currently calls all three tracks "Measured"), regenerate RESULTS.md and results.json via the runner, and re-verify determinism. Wave 2's A9 upgrades the track to genuinely measured; this fix is so the public page never overclaims in the meantime. Acceptance: dash scans clean; regenerated artifacts; no manifest-measurement claim remains until A9 lands.

**A5 (opus): input-first cost default.** `cost.rs` estimates output volume as input times the output:input PRICE ratio; the estimated output is 97 percent of the default displayed total. Change the default to input-side only: when `output_tokens` is None, bill zero output, set `output_estimated` to false, and add a note "Output not included; supply an output token count to model it." Add an explicit opt-in for the old behavior (a `estimate_output: bool` request field; web cost panel gains an off-by-default toggle labeled as a rough volume assumption; CLI gains `--estimate-output`). Update tests (sonnet_basic_no_cache changes), the web panel, the CLI command, and PLAN section 10's wording. WASM binding stays JSON-compatible (additive field). Acceptance: default per-call total equals input-side cost in a live smoke; toggle reproduces the old number with its label; gates green; browser-verify the panel.

**A-R1 (sonnet, web research, read-only, parallel-safe with all of Wave 0):** produce a short memo with URLs for three facts the implementation agents consume: (1) current Gemini API cached-token pricing for the 2.5 family (implicit and explicit context caching) for A2's Gemini decision; (2) the Guzik caveman benchmark range: tolkin's methodology says "9-21 percent" while both researches cite 14-21; fetch the Dev.to source and determine the right citation (if methodology.md needs the correction, hand it to A4 before the bench regeneration); (3) the Tool Search retrieval-accuracy caveat (the Arcade.dev finding of roughly 56 percent regex retrieval accuracy) with a citable URL, for A12's note. No code edits.

### Wave 1: the cache layer (the owner's priority)

**A6 (inherit Fable 5, 1M context): `tolkin cache` slice 1.** Implement the deep-dive spec from REVIEW-FINDINGS verbatim. Summary of the contract: per-request retention in the usage readers (a compact per-session vector of ts, fresh, cache_read, write_5m, write_1h; parse-cache version bumped to v3 with the established self-healing reparse), a pure analysis module computing (1) hit rate with the under-0.5 "likely broken" advisory and citation, (2) write churn (cache_creation events after a session's first write, named worst sessions), (3) the TTL counterfactual: simulate 5m-TTL and 1h-TTL strategies over the real gap timeline (reads refresh the TTL; A7 must verify and cite that semantic), report observed writes versus both strategies and the dollar delta at pinned rates; the break-even statement is that 1h wins when 1.9 x W1 < 1.15 x W5, (4) cadence facts (intra-session gaps over 5m, inter-session gaps under 1h, zero-cache-read sessions). Surfaces: `tolkin cache [--global] [--json]`, an additive `cache` block in `stats --json` measured output, one Spend-tab row in the TUI, one section in `report --html`. Labeling is non-negotiable: gap data and observed volumes are Tier 3 ground truth; every counterfactual is a Tier 1 advisory estimate computed from Tier 3 inputs, and the output says exactly that; the Claude Code scope line (prefix stability and session shape are the levers there; TTL choice is an API-builder lever) prints always. Privacy unchanged: timestamps and token counts only. Hand-computed golden tests over synthetic fixtures; never touch the owner's real data dir in tests (TOLKIN_DATA_DIR temp). Acceptance: an I2-style reconciliation against an independent recompute of this machine's real logs, run twice, exact match both times, recorded in PROGRESS with the table.

**A7 (opus, adversarial reviewer, runs after A6's first complete draft):** orders are to break it. Priorities: the TTL-refresh assumption (verify against current Anthropic prompt-caching docs whether cache reads refresh the 1h TTL the way they refresh the 5m TTL; if not, the simulation changes; pin the verified semantics and citation in the module rustdoc), clock skew and out-of-order records, sessions interleaved across projects, interaction with the cross-file dedup, cache v2-to-v3 migration, counterfactual labeling honesty, and division-by-zero or empty-state edges. Every confirmed break gets a regression test before ship. The wave does not merge until A7 signs off in writing in the PROGRESS entry.

### Wave 2: CI deltas, real MCP measurement, measured advisories

**A8 (opus): delta-versus-baseline CI gates.** Today the action and tolkin-audit.yml report absolute state; the researches' strongest CI idea is the delta. In `distribution/action/` and `.github/workflows/tolkin-audit.yml`: run `tolkin project --json` at the merge base (separate checkout or git worktree) and at HEAD, have `build-report.mjs` render delta columns (always-loaded tokens, context tokens, MCP cold) with signs and percentages, and add inputs `max-always-delta-tokens` and `max-context-delta-pct` that fail the job when exceeded (default: off). Keep the severity gate. Dogfood: open a PR that deliberately adds context weight and capture the red delta row; revert it. Acceptance: action dry-run green; the dogfood PR screenshot or comment text recorded in PROGRESS.

**A9 (inherit Fable 5, 1M context): tools/list ingestion plus the benchmark upgrade.** Two halves, one agent, because they share fixtures. Half one: `tolkin mcp` accepts a tools/list JSON (new flag or auto-detected shape), tokenizes each tool (name, description, input_schema serialized compactly) with the real CLI tokenizers, renders a per-tool table, and lints description smells (no purpose verb, missing output shape, over-long descriptions, near-duplicate descriptions across tools) with the compact-clarity framing from the research (better descriptions can help success but verbose augmentation increases steps; do not recommend longer). Exact counts supersede catalog estimates when supplied; the unknown-server prompt now points at the flag. Additive WASM binding for the web panel. Half two: vendor real public tools/list manifests as benchmark fixtures (license-checked, attributed), convert the configuration track to tokenized-manifest measurement, restore the methodology track-2 manifest claim A4 removed (now true), bump the results schema if fields change (`apps/tolkin-web/src/types/bench.ts`, version field), regenerate, verify determinism, and confirm /bench renders. Acceptance: configuration rows derive from tokenized manifests; per-tool table live-smoked; core and CLI tests; determinism green.

**A10 (sonnet): measured advisories.** Three additions to stats, TUI, and report, all measured-tier, all reconciled against synthetic golden fixtures: (1) model mix: share of priced spend on the top model and the fan-out share, with the research's cited thresholds (over 25 percent of spend on the frontier model suggests over-use; under 10 percent on the small model suggests under-fan-out) rendered as advisories with citations, never as judgments; (2) output share: output tokens and output dollars as a share of the session bill, with one sentence connecting it to the output-compression ceiling ("this is the most an output-side tool could touch"); (3) cap runway: optional `monthly_cap_usd` in config.toml; when set, show days-to-cap from the 7-day and 30-day measured burn rates. Acceptance: golden tests; a real-log smoke on this machine recorded in PROGRESS.

### Wave 3: distribution (Homebrew), hygiene, skills, bench expansion

**A11 (sonnet): Homebrew. Implement the researched spec in section 5 below.** Deliverables: `Formula/tolkin.rb` staged at `distribution/homebrew/Formula/tolkin.rb` (it moves to the tap repo at creation), a packaging-and-release job appended to `tolkin-publish.yml` (tar.gz per platform from the existing build artifacts, SHA256SUMS, `gh release create` on the public repo via the PAT secret, formula bump commit to the tap repo), README install section, support-matrix row. Local validation before any owner action: `brew install --formula ./distribution/homebrew/Formula/tolkin.rb` against a locally staged tarball (file:// URLs in a scratch copy of the formula are fine for the dry run), `brew test tolkin`, `brew audit --strict` (binary-formula audit warnings in a personal tap are acceptable; record which ones fire and why they are fine). Acceptance: formula installs and `tolkin --version` matches; the publish-job YAML passes the duplicate-key and dash scans; owner-action checklist updated.

**A12 (sonnet): hygiene batch.** (a) `MCP_CATALOG_OBSERVED` date constant in mcp.rs, printed in analysis notes and on the web panel (the PRICES_OBSERVED pattern applied to the catalog). (b) scan instruction catalog additions: `.windsurf/rules/` and `.clinerules`, per-OS path tests. (c) a `.github/workflows` detector that flags LLM-invoking workflow steps (claude-code-action and friends) and counts their prompt-bearing fields, reported as its own bucket, never folded into always-loaded. (d) `tolkin stats --json` with an empty ledger emits valid JSON (an empty-state object with hints in a field), not prose. (e) the Tool-Search accuracy caveat (from A-R1's memo) added to the mcp Tool-Search note. (f) denominator sweep: every rendered percentage names its denominator (of always-loaded context, of a 200K window, of input-side tokens) across CLI, web, report, and the action comment. Acceptance: tests per item; gates.

**A13 (sonnet): the cache skill, hook templates, privacy doc.** (a) `distribution/skills/tolkin-cache/SKILL.md`: wraps `tolkin cache --json`, teaches the tier-correct reading of counterfactuals, applies prefix-stability fixes only with confirmation, re-measures; schema blocks generated from live output and covered by A3's drift lint. (b) Hook recommendations: scan gains detection of hook config files, and the recommendations output offers cited, copy-pasteable PreToolUse guard and PostToolUse truncation templates; never auto-installed. (c) `distribution/PRIVACY.md`: the posture, the env kills (CI, TOLKIN_NO_LEDGER, TOLKIN_DATA_DIR), what the ledger stores, what ingestion reads, the one sanctioned fetch. Acceptance: drift lint green including the new skill; dash scans.

**A14 (opus): benchmark expansion.** Verify more public claims on declared fixtures where headless-runnable, with licenses vendored: notion-slim (claimed ~52 percent), Repomix --compress (claimed ~70 percent), cavemem (claimed ~46 percent, unverified per the research). Anything not headless-runnable gets a status row with the reason, per the established contract. Plus one scored lossy run via the BYOK extraction-QA harness so the public page can show scored=true at least once; this needs the owner's ANTHROPIC_API_KEY (owner gate; if unavailable, ship the rows harness-ready and leave scored=false with the existing disclosure). Mirror updated results into `distribution/benchmarks/`. Acceptance: determinism; every new comparison row carries fixture, status, reason; no number without a runnable basis.

## 5. Homebrew distribution spec (researched 2026-06-10; implement, do not re-research except where marked)

Facts established against current Homebrew docs and registries:

- The name `tolkin` is unclaimed: formulae.brew.sh API returns 404 for both formula and cask, and `brew search tolkin` finds nothing (only tokei). Because there is no core collision, once a user taps, plain `brew install tolkin` resolves to the tap's formula.
- Tap repos MUST be named with the `homebrew-` prefix: create `agnelnieves/homebrew-tolkin`. Users then run `brew tap agnelnieves/tolkin` followed by `brew install tolkin`, or the one-shot `brew install agnelnieves/tolkin/tolkin`. (The user-facing command is `brew install tolkin`, not `brew tolkin`; document the real UX.)
- Bare, no-tap `brew install tolkin` requires homebrew-core, which is out of reach today and that is fine: core explicitly rejects binary-only formulae, requires a DFSG-compatible open-source build-from-source, and applies notability thresholds that triple for self-submission (roughly 90 forks or 90 watchers or 225 stars). Record homebrew-core as the post-OSS-extraction goal (it aligns with PLAN section 11's "via a tap once the project hits 1.0"); the tap ships now.
- Homebrew supports macOS arm64 and x64 (Tier 1) and Linux x64 AND arm64 (both Tier 1 under current support tiers), so the formula covers all four non-Windows tolkin platforms. Windows users stay on the npx path (a scoop bucket is the future analog; parked, not in scope).

Design (two public repos, each with one job):

1. **Binary host: the public companion repo** (`agnelnieves/tolkin`, created from `distribution/`; already a pending owner action). GitHub Releases on it hold the per-platform artifacts: `tolkin-v<version>-darwin-arm64.tar.gz`, `-darwin-x64`, `-linux-x64`, `-linux-arm64` (each containing the single `tolkin` binary, executable bit preserved), plus `SHA256SUMS`. Rationale: release assets on a public repo are publicly downloadable (the private monorepo's releases are not), the companion repo is already the public artifact home for skills and the action, and the formula can point anywhere public, so this choice is reversible. Fallback if the companion repo's creation slips: attach the releases to `homebrew-tolkin` itself; the formula changes only its URLs.
2. **Tap: `agnelnieves/homebrew-tolkin`** containing `Formula/tolkin.rb` and a one-paragraph README. Formula shape (binary formula, standard for personal taps):

```ruby
class Tolkin < Formula
  desc "Privacy-first AI token analyzer for agent workflows"
  homepage "https://github.com/agnelnieves/tolkin"
  version "0.10.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/agnelnieves/tolkin/releases/download/v0.10.0/tolkin-v0.10.0-darwin-arm64.tar.gz"
      sha256 "<sha256>"
    end
    on_intel do
      url ".../tolkin-v0.10.0-darwin-x64.tar.gz"
      sha256 "<sha256>"
    end
  end

  on_linux do
    on_arm do
      url ".../tolkin-v0.10.0-linux-arm64.tar.gz"
      sha256 "<sha256>"
    end
    on_intel do
      url ".../tolkin-v0.10.0-linux-x64.tar.gz"
      sha256 "<sha256>"
    end
  end

  def install
    bin.install "tolkin"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/tolkin --version")
  end
end
```

3. **Automation in `tolkin-publish.yml`** (the private repo's existing workflow already builds all five binaries as artifacts behind a version gate): a new job, after the npm publish succeeds, that (a) downloads the four non-Windows artifacts, (b) tars them with the naming above and writes SHA256SUMS, (c) creates the GitHub Release on the public repo: `gh release create v<version> --repo agnelnieves/tolkin <assets>` authenticated by a new repo secret `HOMEBREW_TAP_TOKEN` (a fine-grained PAT with contents read/write on `agnelnieves/tolkin` and `agnelnieves/homebrew-tolkin`, nothing else), and (d) updates `Formula/tolkin.rb` in the tap (substitute version and the four sha256 values, commit via the same PAT). No third-party marketplace action for the bump; a few lines of gh and sed keep the supply-chain surface at zero, matching the repo's posture. The job is continue-on-error until the owner confirms both repos and the secret exist (the established first-publish pattern).
4. **Notes recorded for the formula:** curl-fetched files do not receive the Gatekeeper quarantine attribute, so the unsigned binaries run fine from the terminal (same posture as the npm path); codesigning and notarization are a nice-to-have for later, not a blocker. Add an optional `livecheck` block pointing at the release tags once the first release exists.

## 6. Verification and escalation

- Any change to a money number or a multiplier requires two independent confirmations before merge (a unit test computing it from the pricing table, and a live CLI smoke), the A2 pattern.
- Adversarial review (a different agent, opus) is mandatory for Wave 1 and for A9's measurement claims.
- If a research memo (A-R1) contradicts REVIEW-FINDINGS, current provider documentation wins; record the correction in PROGRESS with the URL.
- Never relitigate IMPROVEMENTS section 4. If a task seems to require it, stop and surface the conflict instead.
- If a wave's gates cannot go green without weakening a gate, the wave does not ship; write up the blocker in PROGRESS and continue with independent work.

## 7. Owner actions (the only stop points; batch your requests)

1. Create the public companion repo `agnelnieves/tolkin` from `distribution/` (pre-existing action from I5; now also the binary release host; replace the three `<public-repo>` placeholders; tag the action v1).
2. Create `agnelnieves/homebrew-tolkin` (empty is fine; A11's formula and README get pushed there).
3. Create the fine-grained PAT and add it as the `HOMEBREW_TAP_TOKEN` secret on the private repo (contents read/write on the two public repos only).
4. ANTHROPIC_API_KEY repo secret (unblocks the weekly drift watchdog, carried over, and A14's one scored lossy run).
5. Confirm the six npm Trusted Publishers so the remaining publish legs go strict (carried over).
6. One human pass over the TUI in a real terminal (carried over; A6 adds a Spend row, so fold it into that pass).

Everything else is yours. Begin with the required reading, then dispatch Wave 0.
