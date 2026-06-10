# Tokler Improvements: Execution Prompt

Copy everything below the line into a fresh agent session to execute the IMPROVEMENTS plan. It is self-contained: context bootstrap, binding rules, phase specs with acceptance gates, the model and effort policy, and escalation rules.

---

You are executing the Tokler Improvements plan (phases I1 through I5) in the monorepo at `/Users/agnel/Documents/agnel-website`, branch `feat/tokenly`. Tokler is a privacy-first AI token analyzer: a Rust core compiled to WASM and native, a CLI published on npm as `tokler-cli` (five platform packages, OIDC trusted publishing), and a Next.js web app. The product is real and live at 0.3.0; you are building its measurement, experience, and distribution layer. Accuracy and output quality outrank speed. When accuracy and scope conflict, cut scope, never accuracy.

## Bootstrap (do this before any work, in this order)

1. Read `/Users/agnel/Documents/agnel-website/CLAUDE.md` (monorepo rules).
2. Read `/Users/agnel/Documents/agnel-website/apps/tokler-web/CLAUDE.md` (Tokler operating rules; AGENTS.md is its mirror).
3. Read `/Users/agnel/Documents/agnel-website/apps/tokler-web/IMPROVEMENTS.md` end to end. This is YOUR plan. Sections 3 (workstreams), 4 (locked decisions), 5 (phasing) are binding.
4. Read `/Users/agnel/Documents/agnel-website/apps/tokler-web/PLAN.md` sections 2, 5, 8, 9, 11 (core architecture you must not violate).
5. Read the last three entries of `/Users/agnel/Documents/agnel-website/apps/tokler-web/PROGRESS.md` (current state) and skim `HANDOFF.md` (gotchas list; the wasm-opt flags, the fmt --all quirk, the JSONC sanitizer, the example.com suppression).
6. Run the gate suite once to confirm a green baseline before changing anything (commands in "Gates" below).

## Binding rules (violations are rework, no exceptions)

1. NO em-dashes or en-dashes in any file you write: code comments, UI copy, docs, commit messages, YAML. Use periods, commas, parentheses, colons.
2. NO AI attribution anywhere: no Co-Authored-By, no "generated with" lines.
3. bun only for JS package operations (`bun add`, `bun install --frozen-lockfile`); never npm/pnpm/yarn install. Run `bun pm untrusted` after every add; never extend trustedDependencies without flagging it.
4. Privacy posture: zero network egress of user content. The only sanctioned fetch is the existing opt-in BYOK verify in `apps/tokler-web/src/lib/verify/`. The new ledger and log ingestion are LOCAL ONLY, consented at onboarding, resettable, disabled when `CI=true` or `TOKLER_NO_LEDGER=1`.
5. Honesty tiers: every savings number surfaced is labeled Identified, Realized, or Measured per IMPROVEMENTS section W2c. Never conflate them. Input-token bounded claims everywhere ("output may vary").
6. Rust style: edition 2021, anyhow errors, clap derive, snake_case serde, JSON-string WASM bindings, release profile untouched (`opt-level = "z"`, lto, strip, panic abort). Comments only where the WHY is non-obvious.
7. Secret outputs: kinds and counts only, never values, spans, or redacted bytes.
8. Update `PROGRESS.md` at the end of every phase (dated entry plus Status table row), bump the version with `bun run --filter=tokler-cli bump`, commit with a conventional message, push. Merging to main triggers the npm publish; only merge when the phase gate is fully green.
9. Do not touch `apps/web` (the personal site) or `apps/cli` (the portfolio CLI) except to read patterns.
10. Never link or reference the private GitHub repo in anything public-facing (skills, action README, benchmark page).

## Model and effort policy (follow this; it is part of the spec)

- Primary harness: Claude Code in the terminal. The repo's conventions, gates, and parallel-agent ritual are built for it. Use Cursor only for interactive human-driven spot edits, with Cursor's strongest available model; never let Cursor agents run multi-file phases unattended.
- Orchestrator session (you): Claude Fable 3 (Mythos-level) when available, `/effort max`; otherwise Claude Opus 4.7 or newer. Plan each phase before implementing it.
- Sub-agents you dispatch:
  - Fable 3 for the two places where reasoning depth pays for itself outright: the W2c tier-accounting math (design plus the second review pass) and the W5 benchmark methodology. These define the product's credibility.
  - Opus-class for the remaining correctness-critical cores: W2b log ingestion (the dedup semantics ARE the feature), the W3 TUI event loop and state model, the W5 benchmark runner implementation.
  - Sonnet-class (Sonnet 4.6 or newer) for well-specified mechanical work: W1 per-OS path tables, Windows packaging legs, npm/package scaffolding, docs mirroring, fixtures.
  - Sonnet-class with web tools for any research verification; escalate synthesis-heavy research to Opus or Fable.
  - Never use a small/fast tier (Haiku-class) for code that ships; acceptable only for trivial lookups.
- Product note while you are in the pricing code: the Fable/Mythos model family is not in tokler-core's pricing table or the drift comparator's model list. If the owner's company adopts it, pricing entries and drift coverage need adding; flag it in PROGRESS.md open questions when you touch pricing.rs.
- Parallelize only disjoint file ownership (the established pattern: each agent brief lists "do NOT touch" areas). Two agents in one file is a merge bug factory.
- Every sub-agent brief must include: required reading list, exact deliverables, the binding rules above, verification commands to run, and "return a summary under 350 words; do not update PROGRESS.md or commit."

## Gates (the full suite; run after every phase, all must pass)

```
cargo test --manifest-path packages/tokler-core/Cargo.toml          # 83+ pass
cargo clippy --manifest-path packages/tokler-core/Cargo.toml --all-targets -- -D warnings
cargo fmt --all --manifest-path packages/tokler-core/Cargo.toml --check    # note: --all required, virtual manifest
cargo test --manifest-path apps/tokler-cli/Cargo.toml               # 48+ pass
cargo clippy --manifest-path apps/tokler-cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path apps/tokler-cli/Cargo.toml --check
(cd packages/tokler-core && wasm-pack build crates/wasm --target web --out-dir ../../pkg --release)
bun run --filter=tokler-web lint && bun run --filter=tokler-web lint:fast
bun run --filter=tokler-web typecheck && bun run --filter=tokler-web build
rg -n "[\x{2014}\x{2013}]" packages/tokler-core/crates apps/tokler-cli/src apps/tokler-cli/scripts apps/tokler-web/src   # zero hits (drift fixtures exempt)
grep -rn "fetch(" apps/tokler-web/src | grep -v "src/lib/verify"    # zero hits
grep -rIn "localStorage|indexedDB" apps/tokler-web/src              # zero hits
bun pm untrusted                                                     # 0
```

Plus per-phase smokes defined below. UI changes additionally need a dev-server SSR check (`bun run --filter=tokler-web dev`, curl the printed port) and an honest note in PROGRESS.md if post-hydration browser verification was not possible.

## Phase specs

Execute strictly in order. One phase = one commit (or a small series) = one PROGRESS entry = one version bump. Do not start a phase until the previous one is merged green.

### I1: Portability + ledger + onboarding

- Per-OS scan catalog: every entry in `apps/tokler-cli/src/scan/mod.rs` gets macOS, Linux, and Windows locations (Claude Desktop: `Library/Application Support/Claude/` vs `~/.config/Claude/` vs `%APPDATA%\Claude\`; VS Code settings likewise). Use `dirs`; key by `cfg(target_os)` or runtime OS detection so tests can inject paths. Unit tests per OS via the existing injected-home pattern.
- Portable bump script: replace BSD `sed -i ''` calls in `apps/tokler-cli/scripts/bump-version.sh` with a portable in-place edit (perl -pi -e is acceptable). Must behave identically on macOS and Linux.
- Windows CI proof: add a `windows-2022` job to `.github/workflows/tokler-ci.yml` running `cargo test` for the CLI. If `tokenizers`' onig feature fails under MSVC, switch the CLI to the crate's pure-Rust feature set and record the decision in PROGRESS.md. (Publish leg comes in I3.)
- Ledger (`apps/tokler-cli/src/ledger/`): `directories::ProjectDirs::from("", "", "tokler")` data dir; append-only `ledger.jsonl` records `{v, ts, command, project_key, headline numbers, tokler_version, prices_observed}`; written by `project`, `mcp`, `scan`, `audit` runs when consent exists. `tokler stats --reset` wipes. `CI=true` or `TOKLER_NO_LEDGER=1` disables silently.
- Onboarding: first TTY run with no config triggers the preflight flow per IMPROVEMENTS W4 (checks with ticks, two consents, command card). `--yes` non-interactive defaults (ledger on, ingestion off); `tokler init` re-runs; config in `config.toml` next to the ledger. Keep it plain prompts, no TUI dependency.
- Acceptance: fresh `HOME` in a temp dir on macOS and Linux (CI) onboards, scans, and writes ledger records; `CI=true` writes nothing; all gates green; windows CI job green.

### I2: Real-usage ingestion + tier accounting + tokler stats

- Reader trait + Claude Code source: parse `~/.claude/projects/*/*.jsonl`; take `message.usage` (input, output, cache_creation, cache_read), `message.model`, `requestId`, `message.id`, `timestamp`, and attribute by the record's `cwd` field (never reverse-map the directory slug). Dedup on (`message.id`, `requestId`) keeping the LAST occurrence (intermediate streaming snapshots grow output_tokens; first-wins undercounts). Exclude `"model":"<synthetic>"` records. Cost via `tokler_core::pricing` (cache-aware), labeled with PRICES_OBSERVED.
- Codex source: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` `token_count` events (`total_token_usage` and `last_token_usage`). Same trait.
- Ingestion is pull-based at command time (no daemon): `tokler stats` reads logs fresh, caches parsed aggregates in the ledger dir keyed by file mtime+size.
- Tier accounting per IMPROVEMENTS W2c: Identified (latest run numbers), Realized (project context-weight delta between ledger snapshots times sessions observed from logs since the delta; fall back to the configured session rate, labeled "assumed rate"), Measured (log aggregates). Implement as a pure module with exhaustive unit tests on fixture ledgers and fixture logs; this math is the product, get it reviewed twice.
- `tokler stats [--global] [--json] [--reset]`: plain table default (current project), `--global` machine-wide, `--json` the full structured form for agents.
- Acceptance: on the dev machine, `tokler stats --json` produces tiered numbers that reconcile by hand against a sample of the real Claude Code logs (document one worked example in PROGRESS.md); fixture-based tests cover dedup (streaming duplicate), synthetic exclusion, cwd attribution, empty-logs, and rate-fallback paths; gates green.

### I3: TUI dashboard + Windows publish

- Ratatui dashboard (ratatui 0.29 + crossterm 0.28, matching `apps/cli` whose render.rs is the in-repo pattern reference; copy patterns, not code). Bare `tokler` in a TTY opens it; non-TTY prints help exactly as today; `tokler stats --tui` is the explicit form.
- Tabs: Project (load-profile bars, top files, identified savings, realized sparkline), Machine (projects ranked by standing weight, totals), Spend (only when ingestion consented: daily bars, model breakdown, cache hit rate). Tab/arrow navigation, q quits, r refreshes. `--compact` renders one static frame to stdout and exits (screenshot artifact).
- Keep the event loop simple: synchronous redraw on input or 1s tick; no async runtime addition.
- Windows publish leg: `windows-2022` matrix entry in `tokler-publish.yml` producing `tokler-win32-x64` npm package (`os: ["win32"]`), launcher map entry including `.exe` suffix handling, bump-script carrier added. First publish of the new package will need the manual-then-trusted-publisher dance; note it for the owner in PROGRESS.md.
- Acceptance: dashboard runs on this machine against real ledger data (terminal smoke; capture `--compact` output into PROGRESS.md), degrades cleanly with an empty ledger; windows binary builds in CI and `tokler --version` runs on the windows runner; gates green.

### I4: Benchmark harness + results page

- `benchmarks/` at `apps/tokler-cli/benchmarks/` (or repo root if cleaner; decide and record): original fixtures per track (structural, configuration, lossy), a runner (bun script or Rust bin) that shells the release tokler binary, emits `results.json` plus a generated `RESULTS.md` with the methodology preamble per IMPROVEMENTS W5 (declared baselines, injection costs counted, tokenizer named per number, date-stamped pricing, N runs).
- Tracks: structural (exact before/after across the audit format previews), configuration (catalog manifests: cold/slim/swap deltas; include a caveman-shrink comparison row if its middleware is runnable headlessly, else document why not), lossy (LLMLingua-2 at 0.7/0.5/0.33 vs wilpel caveman scripts on the same inputs; ratios always, BYOK quality scoring stubbed behind a flag, default off, RCT caveat printed).
- `tokler-bench.yml` workflow_dispatch workflow running the harness and uploading results as an artifact.
- Web: a `/bench` page on tokler-web rendering `results.json` (imported statically at build, client-side rendering, no fetch), with the methodology text and tier/fidelity labels.
- Acceptance: `RESULTS.md` regenerates deterministically (two consecutive runs differ only in timestamps), every number traces to a fixture and a tokenizer, page builds and SSR-renders; gates green.

### I5: Distribution (public companion repo + skills + plugin + action + report)

- `tokler report --html [--global]`: self-contained static HTML report (inline CSS, no JS dependencies fetched) with load profiles, tier-labeled savings, spend trends when available, methodology footnotes. Print-friendly. This part lives in the private repo and ships in the CLI.
- Public companion repo content, prepared IN THIS REPO under `distribution/` for the owner to push to a new public repo (you cannot create the public repo; stage it and stop): root `skills/tokler-audit/SKILL.md`, `skills/tokler-slim/SKILL.md`, `skills/tokler-optimize/SKILL.md` (frontmatter per the vercel-labs skills convention; bodies teach the agent to run tokler-cli via npx/bunx, parse `--json`, apply MCP slim snippets, re-run to verify, and report savings with tier labels); `.claude-plugin/plugin.json` + `marketplace.json`; `action/` composite GitHub Action wrapping `npx tokler-cli@<pinned>` doing the Infracost-style sticky PR comment (generalize the existing `tokler-audit.yml` logic; inputs: fail-on, working-directory, comment-mode); a README for the public repo with zero private-repo references.
- Acceptance: skills validate against the conventions (frontmatter parses, names match dirs), the action runs end to end inside THIS repo's CI as a dry run (workflow_dispatch harness invoking the local action path), `tokler report --html` output opens correctly in a browser (attach the file in the session for the owner); gates green.

## Escalation rules (stop and ask the owner instead of guessing)

1. Any new dependency beyond those named in IMPROVEMENTS/this prompt (dirs, directories, ratatui, crossterm are pre-approved; everything else is a question).
2. Any change to the privacy posture wording, the consent flow semantics, or anything that writes outside the tokler data dir.
3. Windows toolchain fights lasting more than two focused attempts.
4. Anything requiring the owner's accounts: npmjs trusted publishers, creating the public repo, repo secrets.
5. A gate that cannot be made green without weakening it.

## Definition of done for the whole engagement

All five phases merged to main and published, PROGRESS.md telling the story phase by phase, `npx tokler-cli` onboarding a fresh user on macOS, Linux, and Windows, `tokler` opening a dashboard with honest tiered numbers, a regenerable benchmark with a live page, and a staged `distribution/` directory ready to become the public repo. Quality bar throughout: the kind of output you would publish under your own name.
