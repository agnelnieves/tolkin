# Tolkin Progress Log

This is the canonical work log for Tolkin. Every work unit (session, agent run, PR) appends a dated entry to the Log section with what changed and what is next. The Status and Workspace tables at the top reflect current state.

## Status

| Phase | Status | Notes |
|---|---|---|
| 0. Foundation | completed | scaffolding + WASM integration verified 2026-06-09 |
| 1a. Multi-provider tokenization | completed | OpenAI exact, Gemini exact, Anthropic approximated, live in CLI and web 2026-06-09 |
| 1b. File ingestion + tokenization visualizer | completed | file ingestion (PDF/DOCX/XLSX) + token-chip visualizer, live in CLI and web 2026-06-09. Pricing tables and Claude approximator deferred to 1c |
| 1c. Pricing tables + secret redaction + cost calculator UI | completed | pricing/cost/redaction in tolkin-core (23 tests), CLI `redact` + `cost`, web redaction-runs-first + cost calculator. WASM runtime browser-verified 2026-06-09 |
| 2a. MCP analyzer (the wedge) | completed | client-agnostic config parsing + CLI-swap savings in tolkin-core (10 tests), CLI `mcp`, web MCP analyzer UI. Browser-verified 2026-06-09 |
| 2b. Audit rules, hybrid verify, drift comparator | completed | audit engine in tolkin-core (6 production-proven rules, 18 tests), CLI `audit` + `drift` + `count --verify`, web audit panel + Verify with Anthropic. SSR-verified 2026-06-10; post-hydration browser pass pending |
| 3. Frontier (LLMLingua-2 preview, experimental rules, format previews) | completed | 6 experimental audit rules + format previews in core (74 tests), LLMLingua-2 in-browser compression preview (TinyBERT, ~58 MB opt-in download). SSR-verified 2026-06-10. Model2Vec deferred |
| 4. Distribution and community (OSS, GitHub Action) | pending | |
| I1. Portability + ledger + onboarding | completed | per-OS scan catalog, portable bump script, windows CI job, local savings ledger, first-run onboarding, `init` + `stats` commands. 2026-06-10, 0.4.0 |
| I2. Log ingestion + tier accounting + stats | completed | Claude Code + Codex readers (last-wins dedup), three-tier accounting, `tolkin stats --global/--json`. Reconciled token-for-token against real logs. 2026-06-10, 0.5.0 |
| I3. TUI dashboard + Windows publish | completed | Ratatui dashboard (Project/Machine/Spend tabs, `--compact`), bare `tolkin` opens it in a TTY, windows-2022 publish leg + tolkin-win32-x64 package. 2026-06-10, 0.6.0 |
| I4. Benchmark harness + results page | completed | three-track harness (structural, configuration, lossy), methodology doc, deterministic results.json + RESULTS.md, /bench page, tolkin-bench.yml. 2026-06-10, 0.7.0 |
| I5. Distribution layer (skills, plugin, action, report) | completed | tolkin report --html, distribution/ staging (3 skills, plugin manifests, composite action, public README), action dry-run workflow. 2026-06-10, 0.8.0 |
| I6-1. Review hotfixes (P0s + P1s) | completed | audit workflow un-parse-killed, cache multipliers derived from pricing (Gemini cached rates modeled), skills regenerated from live contracts + schema-drift lint in CI, distribution truth fixes + configuration-track honesty labels, input-first cost default. 2026-06-10, 0.9.1 |
| I6-2. tolkin cache slice 1 | completed | per-request retention (parse cache v3), cache_analysis module (hit rate + advisory, write churn, TTL counterfactual at marginal rates, cadence), `tolkin cache` + stats/TUI/report surfaces, adversarial review signed off, I2-style double reconciliation exact. 2026-06-10, 0.10.0 |

## Workspace state

| Workspace | Exists | Builds | Lints | Notes |
|---|---|---|---|---|
| packages/tolkin-core | yes | yes (cargo + wasm-pack) | yes (clippy + fmt) | Rust workspace with core (rlib) + wasm (cdylib). Modules: `pricing` (11 models + `PRICES_OBSERVED`; Gemini 2.5 cached rates modeled), `cost` (cache/batch/long-context; input-side default, output estimate opt-in), `redact` (vendor catalog + entropy + ledger), `mcp` (client-agnostic config analyzer + CLI-swap catalog; cache multipliers derived from the pricing table), `audit` (6 production-proven + 6 experimental rules, format previews), `format` (json-minify, json-to-toon, html-to-markdown). 90 unit tests. WASM bindings expose `redact`/`cost`/`models`/`analyze_mcp`/`audit`/`prices_observed`; artifact in pkg/ (gitignored). |
| apps/tolkin-cli | yes | yes (cargo) | yes (clippy + fmt) | Rust binary. 12 subcommands plus the bare-`tolkin` Ratatui dashboard (Project/Machine/Spend tabs; `stats --tui`/`--compact`): `count` (with `--verify`), `compare`, `viz`, `redact`, `cost`, `mcp`, `audit`, `drift`, `scan` (per-OS catalog), `project`, `init`, `stats` (three-tier savings, `--global`/`--json`/`--reset`). Local savings ledger + opt-in usage-log ingestion (Claude Code, Codex; mtime+size parse cache v3 with per-request retention). `cache` command: measured prompt-cache health (hit rate + advisory, write churn, 5m vs 1h TTL counterfactual, cadence), Claude-Code-sourced. Scripts: portable bump (carries the three skill versions), check-skill-schemas.ts (CI drift lint), cache-recompute.ts (reconciliation harness). 161 unit + 16 integration tests. |
| apps/tolkin-web | yes | yes (Turbopack) | yes (Biome + oxlint) | Next.js 16 + Tailwind v4 + tsgo. Home page: shared-state textarea + file dropzone + redaction ledger + 3-provider count panel (with opt-in Verify with Anthropic on the Claude card) + token-chip visualizer + cost calculator (with prices-observed staleness note) + audit panel (Lighthouse-style ranked findings), plus the standalone MCP analyzer. Tokenization and the tolkin-core WASM each run in a dedicated Web Worker; parsers lazy-load. Redaction runs first and feeds every downstream view. The only sanctioned fetch lives in `src/lib/verify/anthropic.ts`. |

## Open questions

Tracked in PLAN.md section 15. Update there, summarize here as they get resolved.

- **Resolved 2026-06-09:** WASM artifact hosting. We expose `pkg/` through the parent workspace `package.json` via `main`/`types`/`exports`. The web app imports `tolkin-core-wasm` and resolves through workspace-protocol into the built `pkg/`.
- **Resolved 2026-06-09:** No root Cargo workspace. Each Rust crate is independent (mirrors `apps/cli`). `apps/tolkin-cli` path-depends on `packages/tolkin-core/crates/core`.
- **Resolved 2026-06-09 (Phase 1c):** Runtime WASM load is browser-verified. Drove the dev server in a real browser: the tolkin-core WASM loads and runs inside the dedicated Web Worker, redaction populates the ledger (openai-key 100%, config-secret 60%), redacted text flows to every downstream view with no raw-secret leak, and the cost calculator computes correct dollar values (GPT-5.4 $0.0046, Sonnet $0.0040, Gemini Pro $0.0039) with zero console errors.
- **Deferred:** `cargo deny check` not run locally because cargo-deny is not installed in this dev environment. `deny.toml` is in place for CI.
- **Deferred:** WASM bundle size optimization. Currently `wasm-opt = ['-O', '--enable-bulk-memory', '--enable-nontrapping-float-to-int']`; the second flag was added in Phase 1c because the cost calculator's float-to-int casts emit `i64.trunc_sat_f64_u`, which the bundled wasm-opt rejects unless the feature is opted in. WASM is ~937 KB (the regex engine in `redact` dominates). Revisit when bundle size matters.
- **Resolved 2026-06-09 (Phase 1a):** Claude offline approximation strategy. Both CLI and web use cl100k_base (via tiktoken-rs in Rust, gpt-tokenizer's per-encoding subpath in JS) as the documented proxy with ~10% error. bpe-lite was the original plan, but it depends on `node:buffer` and `node:fs` and cannot bundle for the browser without a polyfill stack. cl100k_base is what bpe-lite uses under the hood for its `anthropic` provider anyway. Phase 2 introduces the opt-in Anthropic count_tokens hybrid for exact counts.
- **Resolved 2026-06-09 (Phase 1a):** Gemini Gemma vocab source. CLI embeds Xenova/gemma2-tokenizer (17.5 MB tokenizer.json) via `include_bytes!`. The official `google/gemma-2-2b-it` repo is gated and returns 401 unauthenticated; `unsloth/gemma-3-*` ships 32 MB files. Xenova mirror is the smallest public option. Web lazy-loads `Xenova/gemma-tokenizer` from the HF Hub on first Gemini call.
- **Open:** CLI binary size is 25.5 MB on macOS arm64. The embedded Gemma JSON dominates; `pdf-extract` and the office parsers added ~1.5 MB in Phase 1b. Acceptable for now. Phase 4 distribution may switch to first-run download + cache or a re-serialized binary format.
- **Resolved 2026-06-09 (Phase 1b):** Rust PDF library is `pdf-extract` (MIT), not the AGPL `mupdf` binding (PLAN section 15).
- **Resolved 2026-06-09 (Phase 1b):** Web XLSX uses SheetJS CE installed from the official CDN tarball (`xlsx@0.20.3`), not the stale npm `xlsx`. Installs clean with no postinstall.
- **Resolved 2026-06-09 (Phase 1b):** Claude approximator. Kept cl100k_base and dropped the "port bpe-lite" item as a no-op (bpe-lite's anthropic provider is cl100k_base already). Every Claude count is labeled an estimate with a +/-10% band; exact counts wait for the Phase 2 opt-in hybrid.
- **Resolved (count) 2026-06-09 (Phase 1c):** Gemini tokenization now runs in a real browser (count of 48 tokens confirmed on the redacted sample, Gemma model loaded from the HF Hub). The Gemini chip/segment view shares the same `encode()` call and is correct by construction; switching the visualizer to the Gemini tab specifically was not screenshotted.
- **Deferred:** web still lazy-loads the Gemma tokenizer JSON from the HF Hub at runtime; the pdf.js worker, by contrast, is now bundled locally. Bundling Gemma is still open (PLAN section 15).
- **Resolved 2026-06-10 (Phase 2b):** pricing staleness. `pricing::PRICES_OBSERVED` ("2026-06") now surfaces in cost notes (CLI and web) and via the `prices_observed()` WASM binding. The `models()` shape was left untouched because the web client parses it directly as an array.
- **Resolved 2026-06-10 (Phase 2b):** the privacy scan now allowlists `apps/tolkin-web/src/lib/verify/` for `fetch(`. That module is the single sanctioned egress: the opt-in, BYOK Anthropic count_tokens verify, which sends only redacted text. Scan form: `grep -rn "fetch(" apps/tolkin-web/src | grep -v "src/lib/verify"` must return zero hits.
- **Resolved 2026-06-10 (Phase 2b):** drift scope. Claude version-to-version drift cannot be measured offline (closed tokenizer); `tolkin drift` reports exact OpenAI encoding drift (cl100k vs o200k), the published Opus 4.7 1.0x-1.35x band labeled "not measured," and points at the count_tokens verify for exact per-version counts. Gemini is stated as no-drift (shared Gemma vocabulary).
- **Pending:** post-hydration browser verification of the Phase 2b and Phase 3 web surfaces (audit panel results + experimental toggle + previews, Verify with Anthropic flow, compression preview model load). SSR markup verified via dev server + curl on 2026-06-10; no browser-preview tooling was available in the session. Run the preview flow from section 7 of HANDOFF.md next session.
- **Deferred (Phase 3):** Model2Vec static embeddings for semantic dedup. The MinHash near-dup detection already ships in the audit engine; Model2Vec is an accuracy upgrade for the paraphrase case and deserves its own pass with bundle-size measurement.
- **Resolved 2026-06-10 (Phase 3):** LLMLingua-2 integration. `@atjsh/llmlingua-2@2.0.3` (MIT) targets transformers.js v3; our v4 removed three internal tokenizer surfaces it used. Bridged with a ~40-line adapter built on v4 public APIs (`get_vocab` returns a Map in v4, was a plain object in v3), injected through the library's public constructor. Model: `atjsh/llmlingua-2-js-tinybert-meetingbank`, 57.1 MB fp32 ONNX (no quantized export exists), lazy-loaded in a dedicated worker on first compress. TinyBERT is uncased, so compressed output is lowercased; disclosed in the UI.
- **Noted 2026-06-10 (Phase 3):** `@tensorflow/tfjs@4.22.0` added as a required peer of llmlingua-2 (it imports softmax/tensor3d). Kitchen-sink dependency, but it only loads inside the opt-in compress worker, never on first paint. `js-tiktoken@1.0.21` added types-only; at runtime the worker shims its Tiktoken with gpt-tokenizer's o200k encode to avoid a duplicate rank table.
- **Open 2026-06-10 (Phase I2):** the Claude Fable/Mythos model family has no entry in `tolkin-core::pricing` and no coverage in the drift comparator's model list. It is not hypothetical: this machine's real logs carry 107M input-side tokens under `claude-fable-5` (plus 52M under `claude-opus-4-6`, also absent), all currently surfaced as unpriced in `tolkin stats`. When the family's rates are published (or the owner's company adopts it), add pricing entries and extend drift coverage; until then unpriced-but-listed is the honest behavior.

## Log

### 2026-06-09: Phase 0 complete

Plan finalized at `apps/tokler-web/PLAN.md`. PROGRESS.md, CLAUDE.md, AGENTS.md created here as the canonical operating docs.

Three sub-agents dispatched in parallel scaffolded `packages/tokler-core`, `apps/tokler-cli`, `apps/tokler-web`. Coordinator then wired the WASM artifact through to the web app.

#### Deliverables

- `packages/tokler-core/`: Cargo workspace with `crates/core` (rlib, name `tokler-core`) and `crates/wasm` (cdylib, name `tokler-core-wasm`). Single `version() -> String` API. Release profile matches `apps/cli` (`opt-level = "z"`, `lto = true`, etc.). `package.json` exposes the wasm-pack `pkg/` output via `main` / `types` / `exports`. `deny.toml` mirrors `apps/cli/` plus `wasm32-unknown-unknown` target.
- `apps/tokler-cli/`: Rust binary named `tokler`. clap derive CLI with subcommands `count`, `viz`, `audit`, `mcp`, `cost`, `redact`, `drift`, `compare`. Each is a stub that prints "<name>: not implemented yet". Builtin `tokler version` calls `tokler_core::version()` to prove the path dependency.
- `apps/tokler-web/`: Next.js 16.2.6 + React 19.2.6 + Tailwind v4 + Biome + oxlint + tsgo. Minimal hero page. `src/app/core-version.tsx` is a client component that dynamic-imports `tokler-core-wasm`, calls `init()`, calls `version()`, and renders the result. Build passes Turbopack with the WASM dep registered.

#### Verification

| Check | Result |
|---|---|
| `cargo build` (tokler-core) | pass |
| `cargo test` (tokler-core) | pass, 1 test |
| `cargo clippy --all-targets -- -D warnings` (tokler-core) | pass |
| `cargo fmt --check` (tokler-core) | pass |
| `wasm-pack build crates/wasm --target web --out-dir ../../pkg --release` | pass (13 KB wasm) |
| `cargo build` (tokler-cli) | pass |
| `cargo test` (tokler-cli) | pass, 0 tests |
| `cargo clippy --all-targets -- -D warnings` (tokler-cli) | pass |
| `cargo fmt --check` (tokler-cli) | pass |
| `tokler --version` | prints `tokler 0.1.0` |
| `tokler count` | prints `count: not implemented yet` |
| `bun install` | clean, lockfile updated for new workspace |
| `bun run --filter=tokler-web lint` | pass |
| `bun run --filter=tokler-web typecheck` | pass (tsgo) |
| `bun run --filter=tokler-web build` | pass (Turbopack, static pages generated) |
| Root `bun run lint` | pass |

#### Plan corrections recorded during Phase 0

- Renamed product from "Tokenist" to "Tokler" (tokenist.com naming collision).
- Next.js 16, not 15. Matches `apps/web`.
- No root Cargo workspace. Each Rust crate is standalone, matching the `apps/cli` pattern.
- Phase 0 includes a `package.json` in `packages/tokler-core/` that proxies imports of `tokler-core-wasm` into the built `pkg/` directory via the `exports` field.

#### Up next: Phase 1

Highest-priority work per `PLAN.md`:

1. OpenAI tokenization in CLI (`tiktoken-rs`) and web (`gpt-tokenizer`). Wire into `tokler count` and the visualizer.
2. Gemini tokenization (Gemma SentencePiece) in both surfaces.
3. Claude offline approximation (`bpe-lite` port to Rust for the CLI; npm package for web).
4. File parsing primitives: PDF (`pdf-extract` rust + `pdfjs-dist` web), DOCX, MD, YAML, TOML, JSONC.
5. Secret redaction in Rust (gitleaks + secrets-patterns-DB regex catalog + entropy gate).
6. Tokenization visualizer UI (no fabricated Claude tokens).

These are not in scope yet. They land in the next session.

### 2026-06-09: Phase 1a complete (multi-provider tokenization, count-only)

Both surfaces now tokenize across OpenAI (exact), Anthropic (approximated), and Gemini (exact, via Gemma).

#### Deliverables

- **CLI**: `apps/tokler-cli/src/tokenize/mod.rs` plus `apps/tokler-cli/assets/gemma-tokenizer.json` (17.5 MB from `Xenova/gemma2-tokenizer`). `tokler count` and `tokler compare` are fully implemented; both honor `--model {openai|anthropic|gemini}`, `--all`, and `--json`. Stdin input via `-` or omitted FILE arg works. New helper `apps/tokler-cli/src/input.rs` centralizes stdin/file/`-` reading for future commands. Crate dependencies added: `tiktoken-rs = "0.12"`, `tokenizers = "0.23"` (with `onig` + `esaxx_fast` features).
- **Web**: `apps/tokler-web/src/lib/tokenize/{index,openai,anthropic,gemini,types}.ts` plus `apps/tokler-web/src/app/tokenizer-panel.tsx` (client component) and an updated `apps/tokler-web/src/app/page.tsx`. The home page now has the hero up top, a full-width monospace textarea, and a 3-column grid of live counts. Debounced at 100 ms. Gemma model lazy-loads from `Xenova/gemma-tokenizer` on first Gemini call. npm deps added: `gpt-tokenizer@3.4.0`, `@huggingface/transformers@4.2.0`. `bpe-lite` was dropped in favor of `gpt-tokenizer/encoding/cl100k_base` because bpe-lite requires `node:fs` / `node:buffer` and won't run in the browser without a polyfill stack; cl100k_base is what bpe-lite uses under the hood anyway. Surfaced as `~ estimate, cl100k_base, +/- 10%` in the UI.

#### Verification

| Check | Result |
|---|---|
| `cargo build --release` (tokler-cli) | pass, 24 MB binary |
| `cargo test` (tokler-cli) | pass, 6/6 (4 tokenize + 2 provider parse) |
| `cargo clippy --all-targets -- -D warnings` (tokler-cli) | pass |
| `cargo fmt --check` (tokler-cli) | pass |
| `tokler count` smoke ("hello world") | 3 tokens |
| `tokler count --all` smoke | 3-row table, all providers ~3 |
| `tokler compare --json` smoke | valid JSON with chars/bytes/per-provider/estimate flag |
| `bun run --filter=tokler-web lint` | pass (15 files) |
| `bun run --filter=tokler-web typecheck` | pass (tsgo) |
| `bun run --filter=tokler-web build` | pass (Turbopack, 28 s) |
| `bun pm untrusted` | 0 untrusted postinstalls |

#### Up next: Phase 1b

Per `PLAN.md`:

1. File ingestion in the CLI: PDF (`pdf-extract` or `mupdf` Rust binding), DOCX, MD, YAML/TOML/JSONC. Web side: `pdfjs-dist`, `mammoth`, `unified`/`remark`, etc.
2. Tokenization visualizer: colored token-chip UI for OpenAI and Gemini, count-only confidence band for Anthropic.
3. A real Claude offline approximator: port `bpe-lite`-style merges to Rust (or JS native fork that strips the Node deps). Replace the cl100k_base proxy currently labeled as a ~10% estimate.
4. Pricing tables in `tokler-core` so cost can move into the shared crate instead of being recomputed per surface.

Phase 1c (secret redaction, cost calculator UI) follows.

### 2026-06-09: Phase 1b complete (file ingestion + tokenization visualizer)

Both surfaces now ingest documents and visualize token boundaries. Scope notes: pricing tables (PLAN section 10) were deferred to Phase 1c by decision, and the "real Claude approximator" item was dropped as a no-op (see Decisions). Two parallel sub-agents implemented the CLI and web tracks; the coordinator verified the combined tree, fixed inherited style issues, and committed.

#### Deliverables

- **CLI file ingestion**: new `apps/tokler-cli/src/parse/mod.rs`. `parse::extract(path)` sniffs by extension first, then by magic bytes (`infer`), then falls back to reading UTF-8 text. PDF via `pdf-extract`, DOCX via `zip` + `quick-xml` (`<w:t>` runs, newline per `<w:p>`), XLSX via `calamine` (tab-separated cells, blank line between sheets). Wired into `input::read` so `count`, `compare`, and `viz` all benefit. Text and source formats pass through unchanged (we count what the model receives).
- **CLI `tokler viz`**: replaces the stub. OpenAI and Gemini render alternating ANSI 256-color token chips in a TTY with whitespace made visible; falls back to bracketed `⟦tok⟧` plain output when stdout is not a TTY or `NO_COLOR` is set. Anthropic prints the estimate band only and never fabricates chips. `--model`, `--max-tokens` (default 2000), and `--json` supported.
- **`tokenize::segments`**: new `Segment { id, text, start, end }` (byte offsets). OpenAI decodes per token and marks partial-UTF-8 fragments with `▯`; Gemini uses the HF `tokenizers` crate offsets and pieces (`▁` shown as a space); Anthropic returns an empty vec. `segments().len() == count()` for the exact providers (tested).
- **Web tokenization worker**: new `src/lib/tokenize/worker.ts` + `client.ts`. All three tokenizers run in one dedicated Web Worker (Gemma loads at most once), correlated by request id. `lib/tokenize/index.ts` `count()` now routes through the client; `TokenizerPanel` takes `text` as a prop.
- **Web visualizer**: new `src/app/visualizer.tsx`. Provider tabs; OpenAI/Gemini render inline token chips (6-tone alternating backgrounds, visible whitespace, hover shows index + id, 2000-chip cap). Anthropic shows the count, a +/-10% confidence band, the cl100k_base label, an explicit honesty sentence, and a disabled Phase 2 verify CTA. No chips for Claude.
- **Web file ingestion**: new `src/app/file-drop.tsx` + `src/lib/parse/index.ts`. Drag-and-drop or pick a file; text is extracted in-memory and pushed into the shared textarea state. PDF via lazy `pdfjs-dist` (worker pointed at the bundled asset, no CDN), DOCX via lazy `mammoth`, XLSX via lazy SheetJS CE; text formats read verbatim. New `src/app/analyzer.tsx` client wrapper owns the single shared `text` state for the textarea, dropzone, panel, and visualizer; `page.tsx` stays a server component.

#### Verification (coordinator re-ran the full combined tree)

| Check | Result |
|---|---|
| `cargo fmt --check` (tokler-cli) | pass |
| `cargo clippy --all-targets -- -D warnings` (tokler-cli) | pass |
| `cargo test` (tokler-cli) | pass, 17/17 (adds parse + segment tests) |
| `cargo build --release` (tokler-cli) | pass, 25.5 MB binary |
| `tokler viz` smokes (openai chips, gemini json offsets, anthropic band) | pass |
| `tokler count <PLAN.md>` through new parse path | pass |
| `bun run --filter=tokler-web lint` (Biome) | pass, 21 files |
| `bun run --filter=tokler-web lint:fast` (oxlint) | pass, 0/0 |
| `bun run --filter=tokler-web typecheck` (tsgo) | pass |
| `bun run --filter=tokler-web build` (Turbopack) | pass, static prerender |
| `bun pm untrusted` | 0 untrusted postinstalls |
| em/en dash scan (both workspaces) | clean |
| data-egress / storage API scan (web src) | clean |

#### Dependencies added

- CLI (`Cargo.toml`): `pdf-extract`, `calamine`, `zip` (pinned to v7 to unify with calamine), `quick-xml`, `infer`. All MIT and already in the `deny.toml` allow list, so no `deny.toml` change was needed (the full transitive graph was audited; the only copyleft is an `OR`-licensed UEFI-only crate not built for our targets).
- Web (`package.json`): `pdfjs-dist@6.0.227`, `mammoth@1.12.0`, `file-type@22.0.1`, and SheetJS CE `xlsx@0.20.3` via the official CDN tarball. `bun pm untrusted` reports 0; nothing added to `trustedDependencies`.

#### Decisions and deviations

- **Pricing tables deferred to 1c** (user decision). They are only consumed by the cost calculator, which lands in 1c, so building them now would be dead code.
- **"Real Claude approximator" dropped as a no-op.** bpe-lite's `anthropic` provider is cl100k_base under the hood, which is exactly what both surfaces already use, so porting it changes nothing. We keep the cl100k_base proxy, label every Claude count as an estimate with a +/-10% band, and defer exact counts to the Phase 2 opt-in hybrid `count_tokens` call. This matches the Phase 1a finding already recorded above.
- **Web Gemini segments use transformers v4 `tokenize()`**, not `convert_ids_to_tokens()` (removed in the v4 rewrite). Same pipeline and `add_special_tokens: false` semantics as the Phase 1a count path.
- The coordinator cleaned up three inherited em-dashes in `lib/tokenize/openai.ts` and `anthropic.ts` (Phase 1a comments) while editing those files.

#### Up next: Phase 1c

Per PLAN sections 7 and 10: secret redaction (Rust core, runs first, with the ledger UI on web), the cost calculator UI, and the deferred pricing tables in `tokler-core`. Then Phase 2 (the MCP analyzer wedge).

### 2026-06-09: Phase 1c complete (pricing + cost calculator + secret redaction)

All three Phase 1c deliverables landed across both surfaces and were browser-verified end to end. The shared logic lives in `tokler-core` (single source of truth); the CLI links it natively, the web consumes it through new WASM bindings. I implemented the Rust core, the WASM bindings, and the CLI directly; one sub-agent built the web surface against the generated WASM API; I verified the combined tree and drove a real browser.

#### Core (`packages/tokler-core`)

- `pricing.rs`: vendored mid-2026 tables, 11 models across the three providers (Opus 4.8/4.7, Sonnet 4.6, Haiku 4.5; GPT-5.5/5.4/5.4-mini/5.4-nano; Gemini 2.5 Pro/Flash/Flash-Lite). Per-model input/output, cache-read, 5m/1h cache-write, and the Gemini 200K long-context cliff. `is_default` per provider for cross-provider views.
- `cost.rs`: `estimate(CostRequest) -> CostBreakdown`. Models fresh vs cached input, cache-write, output (estimated from the provider output:input ratio unless supplied), the batch 50% discount, the long-context cliff, and a calls multiplier. Carries honesty notes (input-token-bounded; reasoning/thinking bills at the output rate). Input-bounded by design.
- `redact.rs`: always-on secret redactor. Vendor catalog (OpenAI, Anthropic, Google AI, AWS, GitHub, Slack, Stripe, JWT, private keys, DB URIs, Notion, Linear, Figma, Neon, Supabase) plus structural rules (config `key: value`, `--flag value`, Authorization headers) and a generic high-entropy pass. Confidence = 0.4 base + 0.3 vendor regex + 0.2 keyword context + 0.1 entropy; at or above 0.5 auto-redacts, below goes to a review ledger. Deterministic `<REDACTED:kind>` placeholders (no length leak), false-positive suppression (placeholders, UUIDs, example.com, data URIs), overlap resolution. `--strict` raises the bar to 0.7.
- `Provider` enum plus serde. 23 unit tests (pricing defaults, cost math including cache/batch/long-context, redaction of every vendor kind, FP suppression, strict mode, allow-list, JSON round-trips).

#### WASM bindings (`packages/tokler-core/crates/wasm`)

- `redact(text, options_json)`, `cost(request_json)`, `models()`, all JSON-string in and out (no serde-wasm-bindgen dependency). Rebuilt `pkg/` via wasm-pack (~937 KB). Added `--enable-nontrapping-float-to-int` to the wasm-opt flags so the cost calculator's float-to-int casts validate.

#### CLI (`apps/tokler-cli`)

- `tokler redact [FILE] [--strict] [--allow KIND]... [--json]`: redacted text to stdout (pipeable), ledger to stderr; `--json` emits the full result.
- `tokler cost [FILE] [--model ID] [--output-tokens N] [--calls N] [--cache-hit-rate 0..1] [--cache-ttl 5m|1h] [--cache-write-tokens N] [--batch-fraction 0..1] [--json]`: tokenizes the input with each shown model's provider tokenizer, then prices via the core. Default view compares the three provider defaults; `--model` drills into one. Anthropic rows marked `~` (estimate). Added `serde_json` to serialize the core types.
- 17 tests still pass; release binary builds.

#### Web (`apps/tokler-web`)

- Core WASM client: `src/lib/core/{types,worker,client,index}.ts`. A dedicated module Web Worker lazy-inits the WASM once and answers `{ id, op }` requests (forwarding the raw JSON string); the typed main-thread client (SSR-guarded, request-id map, worker-recreate on error) exposes `redact`/`cost`/`models`/`terminate`.
- `src/app/redaction-ledger.tsx`: presentational ledger (kind, redacted/review badge, confidence %, byte span). Privacy-load-bearing: a `Finding` carries no secret bytes and nothing is copyable.
- `src/app/cost-panel.tsx`: cost calculator. Catalog from `models()`, debounced per-provider counts via the reused tokenize client, compare-defaults or single-model select, toggles (cache hit rate, output tokens, calls, batch, cache TTL), per-call and total `$`, cache split, surfaced notes plus the always-on input-bounded line.
- `src/app/analyzer.tsx`: redaction runs first. Debounced redaction of the raw text; `redactOn` toggle (default on); `effectiveText` (redacted, with a raw fallback while loading) feeds the count panel, visualizer, and cost panel; ledger and a "N secret(s) redacted before analysis" banner shown. `page.tsx` untouched.

#### Verification

| Check | Result |
|---|---|
| `cargo test` (tokler-core) | pass, 23/23 |
| `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` (core, both crates) | pass |
| `wasm-pack build` | pass, ~937 KB |
| `cargo test` (tokler-cli) | pass, 17/17 |
| `cargo clippy` + `cargo fmt --check` + `cargo build --release` (cli) | pass |
| CLI smokes: `redact` (+ `--json`, `--strict`, `--allow`), `cost` (defaults, single model, cache, calls, batch, `--json`, unknown-model error) | pass |
| `bun run --filter=tokler-web` lint / lint:fast / typecheck / build | pass (Biome 27 files, oxlint 0/0, tsgo, Turbopack prerender) |
| em/en dash scan (all three workspaces) | clean |
| privacy scan (web: localStorage / indexedDB / fetch) | clean |
| `bun pm untrusted` | 0 |
| **Browser runtime (dev server, real browser)** | **pass: WASM loads in the worker, ledger populates (openai-key 100%, config-secret 60%), redacted text flows downstream with no raw-secret leak, counts recompute (OpenAI 50 / Anthropic ~51 / Gemini 48), cost computes (GPT-5.4 $0.0046, Sonnet $0.0040, Gemini Pro $0.0039), zero console errors** |

#### Dependencies added

- Core (`crates/core`): `regex`, `serde` (derive), `serde_json`. WASM crate: `serde_json`. CLI: `serde_json`. All MIT / Apache-2.0 / Unicode-3.0, already covered by the `deny.toml` allow lists. No web dependencies added (`tokler-core-wasm` was already a workspace dependency); `bun pm untrusted` unaffected.

#### Decisions and deviations

- **Single source of truth honored.** Pricing, cost math, and the secret catalog live only in Rust; the web calls them through WASM and never reimplements them in TS.
- **`--strict` raises (not lowers) the redaction threshold** per PLAN section 11 wording: fewer, higher-precision redactions (0.7 vs 0.5).
- **Redaction and cost share one dedicated core Web Worker** (loads the WASM once). Cost takes only token counts, no secrets; redaction holds the raw text off the main thread, matching the threat model. `terminate()` is exposed for unmount.
- **`example.com` and `EXAMPLE`-bearing strings are suppressed** as placeholders (RFC 2606), which is why a db-uri test uses a non-example host.
- WASM is ~937 KB (the regex engine dominates); bundle-size optimization deferred.

#### Up next

Phase 2 (the wedge): the MCP config analyzer, the audit rules engine, the opt-in Anthropic `count_tokens` hybrid verify, and the tokenizer-drift comparator.

### 2026-06-09: Phase 2a complete (MCP analyzer, the wedge)

The headline feature: paste any agent's MCP config and get the token cost of its tool definitions plus recommendations to swap servers for their official CLI. Client-agnostic by design. I built the core module, the WASM binding, and the CLI directly; one sub-agent built the web UI against the binding; I verified the combined tree and browser-verified the UI.

#### Core (`packages/tokler-core/crates/core/src/mcp.rs`)

- `analyze(config_text, provider) -> McpAnalysis`. A string-aware JSONC sanitizer (strips `//` and `/* */` comments and trailing commas) feeds serde_json, so VS Code / Cursor / Zed configs parse. Detects the client shape automatically: `mcpServers` (Claude Desktop / Claude Code / Cursor / Continue), `mcp.servers` (VS Code settings), `servers` (VS Code / Copilot), `context_servers` (Zed). Handles a string command or a `{ path, args }` command object (Zed), plus URL / SSE transports.
- A curated 22-entry catalog (GitHub, GitLab, git, filesystem, Postgres, Neon, Supabase, Notion, Linear, Slack, Jira, AWS, Kubernetes, Xcode, Figma, Sentry, Brave, Playwright, Puppeteer, fetch, memory, Drive) matched by command / args / name substrings, each with a representative cold-cache token cost, an official CLI alternative (where one exists), a replace / replace-for-ad-hoc / keep recommendation, and the reasoning.
- Per-server scenarios (cold with the provider cache surcharge, warm with the cache discount, a Tool-Search defer-loading stub, raw capacity) and totals (percent of a 200K window, total reclaimable tokens, dollar savings at the provider input rate). Unknown servers are flagged and excluded from totals. 10 unit tests.

#### WASM + CLI

- WASM: `analyze_mcp(config_text, provider)` returns the McpAnalysis JSON. pkg rebuilt.
- CLI: `tokler mcp [FILE] [--provider] [--json]`. Reads a config file or stdin, prints a per-server table (action, tools, cold tokens, CLI swap, savings), totals, a recommendations list with reasoning, and notes. Smoke-tested across Claude / Cursor `mcpServers`, Zed `context_servers` (JSONC via stdin), and all three providers.

#### Web (`apps/tokler-web`)

- Core client extended: an `analyze-mcp` worker op, an `analyzeMcp(configText, provider?)` client method, and the MCP types in `lib/core/types.ts`.
- New `src/app/mcp-analyzer.tsx`: a self-contained panel (its own state) mounted on the home page below the tokenizer. Pre-filled with a sample config so it renders on first paint. Provider select, debounced analysis, a prominent emerald reclaimable-savings headline, the detected client plus totals, a per-server table with recommendation badges and per-server reasoning, and a calm error box on an unparseable config. All math is in the WASM core.

#### Verification

| Check | Result |
|---|---|
| `cargo test` (tokler-core) | pass, 33/33 (adds 10 mcp) |
| `cargo clippy` + `cargo fmt --check` (core, both crates) | pass |
| `cargo clippy` + `cargo fmt --check` (cli) | pass |
| `tokler mcp` smokes (mcpServers, Zed JSONC via stdin, --provider, --json) | pass |
| `bun run --filter=tokler-web` lint / lint:fast / typecheck / build | pass |
| dash + privacy scans | clean |
| **Browser runtime** | **pass: the MCP analyzer renders the sample config analysis (55,000 reclaimable tokens / $0.165, 3 swaps, detected mcpServers, 51% of a 200K window, per-server table with reasoning), matching the CLI exactly** |

#### Decisions

- **No new dependencies.** The analyzer reuses serde_json (already in core). No lockfile change.
- **Catalog is data, not code** (PLAN section 9): representative cold-cache estimates, refreshable. Unknown servers prompt for a tools/list paste rather than guessing.
- **The MCP analyzer is its own surface**, separate from the tokenizer flow. Its output carries server names and estimates only, never env or secret values, so it does not need the redaction pipeline.

#### Up next

Phase 2b: the audit rules engine (production-proven detections, Lighthouse-style ranked findings), the opt-in Anthropic `count_tokens` hybrid verify, and the tokenizer-drift comparator.

### 2026-06-10: Phase 2b complete (audit engine, hybrid verify, drift comparator)

Three parallel sub-agents built the core audit engine (+ WASM + CLI), the Anthropic count_tokens hybrid verify (web + CLI), and the offline drift comparator (CLI); a fourth built the web audit panel against the new binding. The coordinator verified the combined tree, smoked every new surface, and SSR-verified the page.

#### Audit engine (core + CLI + web)

- `crates/core/src/audit.rs`: `audit(text, options) -> AuditReport`. Six production-proven rules per PLAN section 8: near-duplicate-paragraphs (hand-rolled MinHash, 64 hash functions over 5-byte shingles, union-find clustering), json-verbosity (exact savings via serde_json re-serialization), stack-trace-verbosity (Python/JS/Java/Rust frame regexes), volatile-prefix (timestamps/UUIDs in the first 1KB invalidating the cache prefix), sub-cache-threshold, html-content. Findings carry severity, savings range (input tokens), confidence, a production-proven badge, and a citation URL. Reports always carry the honesty notes (input-bounded savings; bytes/4 label when the caller supplied no token count). 17 new tests; no new dependencies.
- CLI `tokler audit [FILE|-] [--severity] [--rule] [--json]` passes a real o200k count as input_tokens. Filters recompute totals.
- Web `audit-panel.tsx`: Lighthouse-style ranked findings below the cost panel, debounced, fed the redacted text plus an exact OpenAI count; details expand to confidence, badge, and citation.

#### Anthropic count_tokens hybrid verify (the only sanctioned egress)

- Web: `src/lib/verify/anthropic.ts` is the single module allowed to fetch with user content. BYOK key in component state only, `anthropic-dangerous-direct-browser-access` header, SHA-256 in-memory cache, only redacted text sent. The Claude card gains a "Verify with Anthropic" affordance with the disclosure line, exact-count display, and the local-vs-API delta. Editing the text reverts to the estimate.
- CLI: `tokler count --verify [--verify-model]` requires ANTHROPIC_API_KEY, runs the core redactor before sending, and labels the verified value in table and JSON output. Uses ureq (the repo's HTTP convention), no new TLS stack.

#### Drift comparator (CLI, offline only)

- `tokler drift [FILE|-] [--json]`: exact OpenAI encoding drift (cl100k_base vs o200k_base with a percent delta), the published Opus 4.7/4.8 1.0x-1.35x band labeled "published estimate, not measured" over the cl100k proxy, and the Gemini no-drift note. Points at the verify path for exact per-version Claude counts. 4 new tests.

#### Pricing staleness

`pricing::PRICES_OBSERVED = "2026-06"` surfaces in cost notes (CLI and web) and the new `prices_observed()` WASM binding; the web cost panel renders "Prices observed 2026-06. Verify against provider pricing pages."

#### Verification

| Check | Result |
|---|---|
| `cargo test` (core) | pass, 51/51 |
| `cargo test` (cli) | pass, 21/21 |
| clippy + fmt (core and cli) | clean |
| wasm-pack release build | pass; d.ts exports audit + prices_observed |
| `bun run --filter=tokler-web` lint / lint:fast / typecheck / build | pass |
| dash scan, storage scan, fetch scan (allowlisting src/lib/verify/) | clean |
| `bun pm untrusted` | 0 |
| CLI smokes (audit fires volatile-prefix; drift table + json; count --verify negative path; cost prints the prices-observed note) | pass |
| SSR via dev server + curl | pass: audit hint, Verify with Anthropic, MCP analyzer all render |
| Post-hydration browser pass | pending (no preview tooling this session); run next session |

#### Up next

Phase 3 (frontier): LLMLingua-2 in-browser compression preview, Model2Vec semantic dedup, experimental detections with confidence badges, format-efficiency previews. Before that, a quick post-hydration browser pass over the Phase 2b surfaces.

### 2026-06-10: Phase 3 complete (experimental rules, format previews, compression preview)

Two parallel sub-agents built the core extensions (experimental detections + format previews) and the LLMLingua-2 web integration; a third wired the audit panel. Model2Vec deferred to its own pass.

#### Experimental detections (core + CLI + web)

- `AuditOptions.include_experimental` (default false) gates six new rules: `filler-phrases`, `repeated-instructions`, `verbose-role-description`, `excessive-few-shot`, `markdown-overhead`, `lost-in-the-middle`. Each carries `badge: "experimental"`, modest confidence (0.4-0.6), and a citation. Reports gain the note "Experimental findings have higher false-positive risk; review before acting." when they run.
- CLI: `tokler audit --experimental` prints `[EXP]` badges and inline previews (capped at 10 lines).
- Web: an "Experimental rules" toggle in the audit panel header re-runs the audit; experimental findings get a violet chip.

#### Format previews (core + CLI + web)

- New `format.rs`: `json_minify` (lossless), `json_to_toon` (uniform flat arrays only, lossy-low-risk with the teach-by-example caveat), `html_to_markdown` (hand-rolled, near-lossless). No new Rust dependencies.
- `Finding.preview` (optional, skip-if-None): json-verbosity attaches the minified preview; html-content attaches the markdown preview and upgrades savings to the measured byte delta; `json-toon-candidate` is a new experimental finding with the TOON preview.
- Web: previews render in the finding details with a fidelity chip, bytes delta, caveat, mono block, and copy button.

#### LLMLingua-2 compression preview (web only, the first beyond-analysis feature)

- `@atjsh/llmlingua-2@2.0.3` (MIT) bridged to transformers.js v4 with a small public-API adapter (the library targets v3). Model `atjsh/llmlingua-2-js-tinybert-meetingbank` (57.1 MB fp32 ONNX) lazy-loads in a dedicated worker on first compress; nothing loads at page paint.
- `compress-panel.tsx`: opt-in card with an explicit "~58 MB download, runs fully locally" disclosure and Load model button, download progress, rate selector (Conservative 0.7 / Balanced 0.5 / Aggressive 0.33 with the can-increase-total-cost caveat), before/after with exact o200k counts, copy button, and the always-visible RCT honesty banner linking arxiv 2603.23525. End-to-end exercised in bun: 101 tokens to 76/58/39 at the three rates.
- New deps (tokler-web only): `@atjsh/llmlingua-2@2.0.3`, `@tensorflow/tfjs@4.22.0` (required peer, loads only in the opt-in worker), `js-tiktoken@1.0.21` (types only, runtime shimmed with gpt-tokenizer). 0 untrusted postinstalls.

#### Verification

| Check | Result |
|---|---|
| `cargo test` (core) | pass, 74/74 (+23) |
| `cargo test` (cli) | pass, 21/21 |
| clippy + fmt (core and cli) | clean |
| wasm-pack release build | pass |
| `bun run --filter=tokler-web` lint / lint:fast / typecheck / build | pass |
| dash, fetch (allowlist verify/), storage scans | clean |
| `bun pm untrusted` | 0 |
| CLI smokes (experimental on/off, TOON candidate, HTML preview with measured savings) | pass |
| SSR via dev server + curl | pass: Compression preview, Load model, Experimental rules all render |
| Post-hydration browser pass | pending (queued with the Phase 2b pass) |

#### Up next

Phase 4 (distribution and community): MIT open-source release prep, the GitHub Action / Bun script running `tokler audit` on PR diffs, the community MCP catalog, the npm wrapper for `bunx @tokler/cli`, and a hosted demo. Before that: the post-hydration browser pass over Phases 2b and 3, and the deferred Model2Vec pass.

### 2026-06-10: Renamed to Tokler; Phase 4a (npm distribution + local config scanner)

Product renamed from Tokenly to Tokler across all workspaces, crates, binaries, docs, and UI copy (commit a5ca477). The npm names tokler, tokler-darwin-arm64, tokler-darwin-x64 were verified unclaimed before the rename.

#### tokler scan (new CLI command)

Read-only local discovery + optimization report, fully offline:

- **MCP configs** discovered across 16 known client locations (Claude Desktop, Claude Code global and project, Cursor, Codex TOML, VS Code, Zed, Continue, Windsurf, Gemini CLI). Each runs through the core MCP analyzer; CLI-swap recommendations are annotated with whether the replacement binary is actually installed (PATH probed via file metadata, no shelling out).
- **Instruction files** (CLAUDE.md global/project, AGENTS.md, .cursorrules, .cursor/rules/, copilot-instructions.md, GEMINI.md) get exact o200k token counts; `--deep` runs the full audit engine per file.
- **Shell configs** (.zshrc and friends) run through the redactor; output reports counts and kind names only, never values or spans.
- **Environment line** lists detected relevant binaries (node, bun, brew, gh, psql, kubectl, ...) which feeds the swap annotations.
- New CLI deps: dirs 5, toml 0.8 (licenses covered by the existing deny.toml allow list). 12 new tests (CLI suite now 33).
- Live scan on this machine found 3 MCP configs (~37.5K cold tokens across Claude Desktop, Claude Code, Cursor) and recommended neonctl swaps worth ~12K tokens/session.

#### npm distribution (the bunx/npx path)

- `apps/tokler-cli/npm/`: wrapper package `tokler` (52-line dependency-free Node launcher, dev-mode sibling fallback, exit-code propagation) plus platform packages `tokler-darwin-arm64` and `tokler-darwin-x64` with the binary embedded (10.1 MB packed each). optionalDependencies pattern per the esbuild model; binaries stay out of git, `build.sh` rebuilds and stages them.
- Smoke-verified through the launcher: `--version`, `count`, `scan --json` all work via `node npm/tokler/bin/tokler.js`.
- Publish order: platform packages first, wrapper last. Publish attempt hit the npm OTP wall (2FA); awaiting an interactive publish by the owner. Once published: `npx tokler scan` / `bunx tokler` work anywhere on macOS.
- The private-repo constraint shaped the design: binaries ship inside npm packages because GitHub release downloads are not possible from a private repo. No package file references the repo.

#### Verification

| Check | Result |
|---|---|
| `cargo test` (core / cli) | pass, 74/74 and 33/33 |
| clippy + fmt (both) | clean |
| web lint / lint:fast / typecheck / build | pass (post-rename) |
| wasm-pack rebuild as tokler_core_wasm | pass |
| npm pack dry-runs (3 packages) | pass |
| Launcher smokes (version, count, scan) | pass |
| `npm publish` | blocked on OTP, owner action required |

#### Remaining Phase 4 to-dos (parked by request)

- [ ] GitHub Action / Bun script running `tokler audit` on PR diffs with a comment
- [ ] OSS release prep (public repo extraction, contribution guide, CI)
- [ ] Community MCP catalog (move the bundled catalog to its own refreshable repo)
- [ ] Hosted demo (still 100% client-side)
- [ ] Linux (x64/arm64) and Windows builds for the npm platform packages
- [ ] Extend the MCP catalog with the unrecognized servers the live scan surfaced
- [ ] Post-hydration browser pass (Phases 2b and 3, still queued)
- [ ] Model2Vec semantic dedup pass (deferred from Phase 3)

### 2026-06-10: Published to npm (tokler-cli 0.1.0)

All three packages are live on the public registry, published by the owner interactively:

- `tokler-darwin-arm64@0.1.0` (10.2 MB packed)
- `tokler-darwin-x64@0.1.0` (10.1 MB packed)
- `tokler-cli@0.1.0` (the wrapper, 2.6 kB)

Naming note: npm rejected the bare `tokler` wrapper name with "too similar to existing package howler" (typosquat protection; the suffixed platform packages passed). The wrapper is `tokler-cli`; the installed command is still `tokler`. `npx tokler-cli`, `bunx tokler-cli`, and `npm i -g tokler-cli` all work.

Verified post-publish from a clean directory against the live registry: `bunx tokler-cli --version` prints `tokler 0.1.0`, `count --all` returns the 3-provider table, and `scan` discovers and analyzes local MCP configs. Supported platforms today: macOS arm64 and x64. Linux and Windows remain on the Phase 4 to-do list.

### 2026-06-10: tokler project (repo-wide audit) and 0.2.0

The command teams run inside a repository to measure and reduce its AI-agent token footprint. Built for the budget-constrained-team use case: skill orchestration repos, code repos, config-heavy projects.

#### tokler project [DIR]

- Gitignore-aware walker (`ignore` crate, Unlicense OR MIT, no symlinks, skips .git/node_modules/target, `--max-file-bytes` cap).
- Classifies agent-context files with a **load profile**, the report's key insight:
  - `always` (every session): CLAUDE.md, AGENTS.md, GEMINI.md, .cursorrules, .cursor/rules/, copilot instructions, skill frontmatter (names + descriptions live in the system prompt registry), repo-local MCP config tool definitions (cold tokens via the MCP analyzer).
  - `on-invocation`: skill bodies (SKILL.md below frontmatter), .claude/commands/, .claude/agents/, codex prompts.
  - `on-demand`: other files inside skill directories, prompts/ markdown.
  - `docs`: root markdown (README etc.), reported with lower emphasis.
- Per file: exact o200k count, audit findings (production-proven; `--experimental` adds the rest), secret stats (kinds and counts only, never values).
- Rollup: weight by profile, always-loaded detail, heaviest files (`--top`), findings by rule, secret flags, totals with reclaimable range.
- `--fail-on high|medium|low` exits 2 when findings at or above the threshold exist: the CI hook for a future GitHub Action.
- Token counting parallelized via std::thread::scope (8-thread cap, deterministic output ordering).
- 15 new tests (CLI suite now 48).

Live run on this monorepo: 454 files scanned, 18 in agent context (~36.2K tokens; always ~6.8K, skill bodies ~27.9K), heaviest file 6,779 tokens, 13 high-entropy secret flags in docs (kinds only).

#### 0.2.0

Version bumped across the crate and all three npm packages; binaries rebuilt and staged; wrapper README documents `project`. Publish (same order: arm64, x64, wrapper) awaits the owner's interactive npm auth.

#### Verification

| Check | Result |
|---|---|
| `cargo test` (cli) | pass, 48/48 (+15) |
| clippy + fmt (cli) | clean |
| npm pack dry-runs (3 packages at 0.2.0) | pass |
| Launcher smokes (`--version` prints 0.2.0, `project` on this monorepo) | pass |
| `--fail-on` semantics (high exits 0 here, low exits 2) | pass |

### 2026-06-10: Version bump script + npm publish workflow; 0.2.1

- `apps/tokler-cli/scripts/bump-version.sh` (also `bun run --filter=tokler-cli bump patch|minor|major|x.y.z`): bumps every version carrier in one shot (Cargo.toml, the Turbo workspace stub, the wrapper package.json including its optionalDependencies pins, both platform package.jsons) and refreshes Cargo.lock via cargo metadata. Verifies all carriers agree before declaring success.
- `.github/workflows/tokler-publish.yml`: publishes to npm on push to main (paths-filtered to tokler-cli and tokler-core). Gated: if the wrapper version in the tree is already on the registry the run is a no-op, so ordinary pushes never publish. Runs on macos-14 (arm64 host, x64 cross-compiled), builds via npm/build.sh, smokes the launcher, publishes platform packages then the wrapper, and verifies from the registry. Per-package guards make re-runs idempotent.
- Requires the NPM_TOKEN repository secret: an npm granular access token with publish permission for tokler-cli, tokler-darwin-arm64, tokler-darwin-x64. Automation tokens bypass the OTP prompt that blocked in-session publishing.
- Ran the bump: 0.2.0 -> 0.2.1 (0.2.0 was never published; the registry sits at 0.1.0 and will jump to 0.2.1 on the first main push). Binaries restaged at 0.2.1; launcher prints tokler 0.2.1.
- Note: this workflow is the publish half of CI. The "tokler audit on PR diffs" action from the Phase 4 to-do list is still separate and parked.

### 2026-06-10: Publish workflow switched to npm Trusted Publishing (OIDC)

npm's token form flags 2FA-bypass tokens as a security risk and recommends Trusted Publishing for CI. Switched: the workflow now authenticates via GitHub's OIDC identity (`id-token: write` permission, npm >= 11.5.1 from node 24). No NPM_TOKEN secret, nothing to expire or rotate, no 2FA bypass.

Owner setup (once, per package, on npmjs.com): package page > Settings > Trusted Publisher > GitHub Actions, with organization `agnelnieves`, repository `agnelweb`, workflow filename `tokler-publish.yml`, environment blank. Required for all three: tokler-cli, tokler-darwin-arm64, tokler-darwin-x64.

Note: provenance attestations require a public source repo, so publishes from this private repo are valid but carry no provenance badge. Revisit at OSS extraction.

### 2026-06-10: Slim-profile recommendations, Linux distribution, PR audit action, CI + drift watchdog; 0.3.0

Four parallel agents (one research, three implementation) plus a wave-2 implementation agent delivered items 2, 3, and 4 of the roadmap in one pass.

#### Slim-profile recommendations (the Jira/Confluence unblock)

- Research pass verified tool-filtering mechanisms against current docs for the full catalog. Highlights: mcp-atlassian supports on-prem Server/DC (custom URL + PAT) and filters via `ENABLED_TOOLS` / `TOOLSETS` / `READ_ONLY_MODE` (68 tools default, 2-5 slim); GitHub MCP via `GITHUB_TOOLSETS` (the dynamic-toolsets flag was removed; never recommend it); Slack (korotovsky) via `SLACK_MCP_ENABLED_TOOLS`; Supabase via hosted URL `features=` params; Kubernetes via `--toolsets`; Brave via space-separated `BRAVE_MCP_ENABLED_TOOLS`; Sentry via `MCP_DISABLE_SKILLS`. Verified no-filtering: Notion, Linear, Figma, Neon, fetch, memory, filesystem, git, Postgres, Drive.
- Core: `CatalogEntry.slim` static data, per-server `SlimRecommendation { already_slimmed, option, est_tokens_saved }` with presence detection over env keys, args flags, and url params. Already-slimmed servers get their cold estimate scaled down and labeled. Totals carry `slim_savings_tokens` / `slim_savings_usd` separately from swap savings (no double counting; swap stays primary, slim is the "if you keep it" fallback). Analysis notes mention the client-side lever (Claude Code tool search on by default in 2.1.x, API defer_loading) for unfilterable servers. 9 new tests (core at 83).
- Catalog corrections: jira entry now states jira-cli (ankitpokhrel) supports on-prem Server/DC (`JIRA_AUTH_TYPE=bearer` + PAT); Confluence on-prem has no mature read CLI (mark is publish-only), making the slim env-var route the primary recommendation there.
- CLI: `tokler mcp` prints copy-pasteable slim snippets per server and a slim-savings totals line; `tokler scan` adds a per-config slim hint. Web: slim badges, expandable mechanism + snippet with copy button, slim figure in the headline.

#### Linux distribution

- New npm platform packages `tokler-linux-x64` and `tokler-linux-arm64`; launcher platform map extended; wrapper optionalDependencies now lists four platforms.
- `tokler-publish.yml` restructured: version gate job, build matrix (macos-14 native arm64 + cross x64; ubuntu-24.04 x64; ubuntu-24.04-arm arm64 with continue-on-error since arm runner availability varies by plan), artifact upload/download, single publish job staging all binaries and publishing platform packages then the wrapper. Trusted Publishing preserved throughout.
- Caveat (flagged, unconfirmed in npm docs): first-ever publish of brand-new packages via OIDC trusted publishing may not be possible since the trusted-publisher setting lives on an existing package's settings page. The two linux publish steps are continue-on-error so the wrapper still ships; if they fail on first run, publish them once manually then configure their trusted publishers.

#### PR-diff audit action

- `.github/workflows/tokler-audit.yml`: on pull_request to main, builds tokler, diffs against the merge base, filters to agent-context files (CLAUDE.md, AGENTS.md, .cursorrules, .claude/, SKILL.md, .mcp.json, root markdown), runs `tokler project --json` plus per-file audits, and upserts a PR comment (marker `<!-- tokler-audit -->`) with a per-file table, load-profile totals, and the input-token honesty footer. Report-only by default; a documented `TOKLER_FAIL_ON` env flips it to gating via `tokler project --fail-on`.

#### CI + drift watchdog

- `.github/workflows/tokler-ci.yml`: core (clippy, fmt --all, test, cargo-deny), cli (same), web (wasm-pack via taiki-e/install-action, pkg build, bun frozen install, all four tokler-web gates). Push/PR on main and feat/tokenly, paths-filtered, plus dispatch. This closes the "tokler crates are not in CI" gap.
- `.github/workflows/tokler-drift.yml` + `apps/tokler-cli/scripts/drift-check.sh` + `apps/tokler-cli/fixtures/drift/` (6 original fixtures covering prose, Rust, Python, JSON, multilingual, symbols/emoji): Mondays 06:00 UTC, compares the cl100k proxy against Anthropic count_tokens per fixture, fails when any drift exceeds DRIFT_THRESHOLD_PCT (default 15) with a reminder that the UI advertises a +/-10 percent band. Requires the ANTHROPIC_API_KEY repo secret; selftest mode (DRIFT_SELFTEST=1) validates plumbing without a key (verified locally, 0.0 drift on all fixtures).

#### Version and verification

- Bump script extended to the two new linux carriers (11 version strings move in lockstep); bumped 0.2.1 -> 0.3.0.
- Gates: core 83/83, CLI 48/48, clippy + fmt clean both, web all four gates pass, YAML lint passes on all four tokler workflows, dash scans clean (emoji confined to the drift fixture by design). Local darwin staging rebuilt at 0.3.0; launcher prints tokler 0.3.0.

#### Owner setup still needed

- Trusted Publisher config on npmjs.com for tokler-cli, tokler-darwin-arm64, tokler-darwin-x64 (repo agnelnieves/agnelweb, workflow tokler-publish.yml), and later for the two linux packages once they first exist.
- ANTHROPIC_API_KEY repo secret for the drift watchdog.

### 2026-06-10: 0.3.0 shipped via Trusted Publishing; linux first-publish pending

The merge to main fired tokler-publish run 27285734187. Outcome: all four build legs succeeded (including the ubuntu-24.04-arm runner), and tokler-cli@0.3.0, tokler-darwin-arm64@0.3.0, and tokler-darwin-x64@0.3.0 published via OIDC trusted publishing, confirming the owner's npmjs configuration works end to end. The two new linux packages hit the predicted first-publish gap (a trusted publisher cannot be configured on a package that does not exist), and the verify step then failed the run by hard-requiring them.

Fixes and follow-ups:
- Verify step patched: strict on the wrapper + darwin packages, warning-only on the linux packages while their publish steps remain continue-on-error.
- The CI-built linux binaries (x64 and arm64 ELF, from the run's artifacts) are staged locally into npm/tokler-linux-*/bin/ for the owner's one-time manual publish. After that, configure trusted publishers for both on npmjs.com (same repo + workflow) and future releases flow automatically.

### 2026-06-10: Linux packages published; IMPROVEMENTS.md planning pass

- Owner manually first-published tokler-linux-x64@0.3.0 and tokler-linux-arm64@0.3.0 (the OIDC first-publish gap). All five platform packages are now live; configure trusted publishers for the two linux packages so future releases flow automatically.
- New planning document: `apps/tokler-web/IMPROVEMENTS.md`. Covers the portability audit (macOS-shaped scan paths, missing Windows builds, BSD-only bump script), the Caveman positioning and a three-track public benchmark design, the local savings ledger + opt-in real-usage ingestion (Claude Code / Codex session logs, verified formats), three-tier savings accounting (identified / realized / measured), the `tokler stats` command + Ratatui dashboard, first-run onboarding, and the distribution layer (public companion repo with skills + Claude Code plugin + GitHub Action + `tokler report --html`). Phases I1-I5. PLAN.md remains authoritative for core architecture.

### 2026-06-10: Blog draft notes and execution prompt

- `apps/tokler-web/BLOG-DRAFT.md`: working notes for a future blog post (story arc one-liners, citable numbers, reference file map, voice/SEO reminders). Not a post; the real draft goes to apps/web/src/content/blog/ under the root CLAUDE.md rules.
- `apps/tokler-web/EXECUTION-PROMPT.md`: a self-contained prompt to hand any agent to execute IMPROVEMENTS.md phases I1-I5: bootstrap read order, binding rules, the full gate suite, per-phase specs with acceptance criteria, an embedded model and effort policy, and escalation rules.

### 2026-06-10: Blog draft repositioned; LESSONS.md added

- Updated `apps/tokler-web/BLOG-DRAFT.md` to clarify the actual blog post will be authored in `apps/web/src/content/blog/`; this file is context-only for the future drafting agent.
- New `apps/tokler-web/LESSONS.md`: dated "what went wrong" log mining PROGRESS for the friction beats (naming/howler, agent connection drop, bun --filter root-pollution, bpe-lite dropped, wasm-opt feature flags, parent/pkg name collision, npm OTP wall, OIDC first-publish gap, TCC sandbox revocation). Future incidents append there; the eventual blog post mines it for honesty.

### 2026-06-10: Phase I1, portability + local ledger + onboarding; 0.4.0

Three parallel agents (per-OS scan catalog, portability tooling, ledger + onboarding core) plus orchestrator integration delivered IMPROVEMENTS phase I1.

#### Per-OS scan catalog

- `scan::ScanRoots { home, cwd, config, data, os }` with `ScanRoots::detect()` (dirs crate) replaces the home/cwd pair. New `Base::Config` resolves against the platform config dir, which collapses most per-OS divergence into one entry: Claude Desktop and VS Code settings are now found on macOS (`~/Library/Application Support`), Linux (`~/.config`), and Windows (`%APPDATA%`) alike. Zed keeps an explicit per-OS split (unix `~/.config/zed/settings.json`, Windows `%APPDATA%\Zed\settings.json`). Home-dotfile clients (Claude Code, Cursor, Codex, Continue, Windsurf, Gemini CLI) are home-based on all three OSes.
- Presence-only helpers `existing_mcp_sources` and `existing_instruction_files` feed the onboarding preflight without tokenizing anything.
- 11 new tests cover per-OS resolution by injecting roots that simulate each OS layout; assertions build paths with PathBuf joins so they pass on Windows too.

#### Portability tooling

- `scripts/bump-version.sh` now uses `perl -pi -e` with env-passed, `\Q...\E`-quoted patterns instead of BSD `sed -i ''`. Round-tripped 0.3.0 -> 9.9.9 -> 0.3.0 with all carriers agreeing.
- `tokler-ci.yml` gains a `cli-windows` job (windows-2022, cargo test only) as the true portability gate. First run fires on this push; if `tokenizers`' onig/esaxx C/C++ fights MSVC, the documented fallback is the crate's pure-Rust set (`fancy-regex`, drop `esaxx_fast`).
- `deny.toml` graph targets now include `x86_64-pc-windows-msvc` so windows-only deps are license-vetted.

#### Local savings ledger (the privacy posture extension)

- Posture change, stated deliberately: from "no persistence" to "local-only, consented, resettable persistence." CLAUDE.md and AGENTS.md carry the wording. Zero network egress, unchanged.
- `src/ledger.rs`: data dir via `directories::ProjectDirs::from("", "", "tokler")` (TOKLER_DATA_DIR override for tests), append-only `ledger.jsonl`, one record per analyzing run: `{v, ts, command, project_key, headline, tokler_version, prices_observed}`. Headline numbers only, never file contents, never secret values. Writes require recorded consent AND no env kill (`CI` truthy or `TOKLER_NO_LEDGER` truthy disable silently; "0"/"false" count as unset). All ledger failures are swallowed: a ledger problem can never break a command.
- Decision: `ts` is unix epoch seconds (u64). No date crate was added; `stats` formats UTC display time with a 12-line civil-from-days helper (Hinnant's algorithm).
- Hooks in `project`, `mcp`, `scan`, `audit` record headline numbers after output; `--json` stdout is untouched.
- New dependency: `directories` 6.0.0 (MPL-2.0, already in the deny.toml allowlist).

#### Onboarding + init + stats

- First TTY run with no config triggers the W4 preflight: banner ("Nothing leaves this machine."), five checks with tick marks (tokenizers load, agent CLIs on PATH, MCP configs per client, instruction files in cwd, session logs detected), two consents (ledger default yes; ingestion offered only when logs exist, default no), then a command card with one preflight-derived suggestion. Non-TTY and env-disabled runs skip silently; `--yes` (global flag) takes defaults without prompting; `tokler init` re-runs on demand. Config lives in `config.toml` next to the ledger (v, consents, onboarded_at, reserved session_rate_per_day).
- `tokler stats`: ledger path, record count, per-command counts, first/last record in UTC; `--reset` deletes ledger.jsonl and keeps config.toml. The `--json`/`--global` forms and tiered numbers land in I2 with ingestion (deliberately deferred so the JSON schema ships once, with tier accounting).

#### Verification

| Check | Result |
|---|---|
| cargo test (cli) | 59 unit + 4 integration, all pass |
| cargo test (core) | 83/83 |
| clippy + fmt, both workspaces | clean |
| wasm-pack build + web lint/lint:fast/typecheck/build | pass |
| dash scan, egress scan, storage scan, bun pm untrusted | clean / clean / clean / 0 |
| Acceptance smoke (fresh HOME + TOKLER_DATA_DIR) | init --yes onboards, scan finds the seeded Cursor config and writes a scan record, stats renders it, stats --reset wipes |
| CI=true negative | fresh data dir stays empty after a scan |

Bumped 0.3.0 -> 0.4.0 (minor: new commands and persistence layer). Windows CI proof rides this push; result to be recorded in a follow-up entry.

### 2026-06-10: Windows CI proof green; cargo-deny gate unbroken

The I1 push proved two things. First, the new `cli-windows` job passed on its first run: the CLI (including `tokenizers` with onig/esaxx C/C++) compiles and tests clean under MSVC, so the documented pure-Rust fallback stays unused. Second, the cargo-deny steps had NEVER been green: every tokler-ci run since the workflow landed (including the 0.3.0-era runs) failed on deny, invisible locally because cargo-deny was not installed on the dev machine. Fixed at the root, no gate weakened:

- Our three crates (`tokler`, `tokler-core`, `tokler-core-wasm`) now declare `license = "MIT"` (per PLAN section 12) and `publish = false` (npm is the distribution channel; flip at OSS extraction). This clears the unlicensed errors and the wasm-pack license warning.
- `allow-wildcard-paths = true` in both deny.tomls: cargo-deny counts a versionless path dependency as a wildcard; ours are private path deps by design.
- `CDLA-Permissive-2.0` allowed in the CLI deny.toml for `webpki-roots` (Mozilla CCADB root-store data license, via ureq/rustls).
- `RUSTSEC-2024-0436` (paste, unmaintained proc-macro via tokenizers) ignored with justification; revisit when tokenizers drops it.
- cargo-deny installed locally (brew) and added to the local gate ritual so this class of drift cannot hide again.

### 2026-06-10: 0.4.0 shipped; linux trusted publishing confirmed end to end

Merge to main fired tokler-publish: version gate, all three build legs, and the publish job green. tokler-cli, both darwin packages, and BOTH linux packages published 0.4.0 via OIDC trusted publishing (the owner's linux trusted-publisher config works; the first-publish gap is fully behind us). Registry verified: `tokler-cli@0.4.0`, `tokler-linux-x64@0.4.0`.

### 2026-06-10: Phase I2, usage-log ingestion + three-tier accounting + tokler stats; 0.5.0

Two parallel agents (log ingestion; tier accounting) plus orchestrator wiring and an adversarial second-pass review delivered IMPROVEMENTS phase I2. The dedup semantics and the tier math are the product here; both were reconciled by hand against this machine's real logs before shipping.

#### Usage-log ingestion (`src/usage/`)

- Reader per source behind shared types. Claude Code: `~/.claude/projects/*/*.jsonl`, attribute by each record's `cwd` (never the directory slug), dedup on (`message.id`, `requestId`) keeping the LAST occurrence (streaming writes growing intermediate snapshots; first-wins undercounts, the known ccusage bug), exclude `"<synthetic>"`, cache writes split 5m/1h from the nested `cache_creation` object. One file can yield multiple sessions (one per distinct cwd). Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, cwd from session meta, per-event `last_token_usage` accumulation (chosen over the final cumulative total because it buckets per day; on local rollouts the two agree within 1.9 percent, fixture-asserted), `info: null` events skipped, fresh input = input minus cached.
- Pull-based at command time, no daemon: an mtime+size parse cache (`usage-cache.json`, versioned, silently self-healing) lives next to the ledger. `stats --reset` clears it along with the ledger.
- Privacy: read-only, token counts and timestamps only, never message content; fixtures are synthetic; ingestion runs only with recorded consent and never under CI/TOKLER_NO_LEDGER.
- Cost: `normalize_model_id` (hyphenated minors to dots, date suffixes stripped) then cache-aware pricing from `tokler_core::pricing`. Unknown ids are NEVER guessed; they surface as unpriced with their volume.

#### Three-tier accounting (`src/tiers.rs`, pure module)

- Identified (advisory estimate): latest project / scan-or-mcp / audit ledger records per scope; scan categorically supersedes mcp to avoid double counting.
- Realized (measured structure, estimated frequency): per project, baseline = first snapshot's always-loaded weight; each session started at or after that snapshot contributes (baseline minus the weight standing at its start). Signed: context growth reports negative, never clamped. Assumed mode integrates the same formula over snapshot intervals at the configured rate (default 10/day, labeled "assumed"). The full formula ships in the module rustdoc as the audit trail.
- Measured (ground truth): session totals, by-model with costs, cache hit rate = cache reads over input-side tokens, unpriced models listed with volume and excluded from the cost sum.
- 18 hand-computed golden tests (single drop, multi-drop windows, growth-then-drop, pre-baseline exclusion, assumed-mode integral, global aggregation, scope isolation, unpriced models).

#### tokler stats (rewired)

- Default scope: the current project (canonicalized cwd, matching ledger keys). `--global` machine-wide, `--json` the full structured report (stable schema; absent tiers serialize as null), `--reset` wipes ledger + parse cache, config survives. Plain output prints every tier with its label and the input-token-bounded note.

#### Worked example (the acceptance reconciliation, real data)

Global measured tier vs an independent Python implementation of the spec (dedup last-wins, synthetic exclusion, cache splits, codex per-event accumulation), run over this machine's real logs. Exact match on every field:

| Field | tokler stats --json | independent recompute |
|---|---|---|
| sessions | 169 (167 Claude Code + 2 Codex) | 169 |
| input_tokens (fresh) | 1,967,219 | 1,967,219 |
| output_tokens | 9,227,681 | 9,227,681 |
| cache_read_tokens | 1,676,359,510 | 1,676,359,510 |
| cache_write 5m / 1h | 0 / 43,868,632 | 0 / 43,868,632 |
| cache_hit_rate | 0.9734 | 0.9734 |

(Point-in-time snapshot; the logs grow continuously because the working session itself writes to them. The reconciliation was run twice, before and after the review fixes below, matching exactly both times. Priced cost at this snapshot: $1,357.22, with claude-fable-5 and claude-opus-4-6 listed unpriced.)

Realized tier reconciliation: seeded ledger with two snapshots for this repo (always 12,000 at 2026-06-01, 8,000 at 2026-06-05). Real logs show 8 sessions of this repo started at or after the baseline; 2 ran in the baseline window (contribute 0), 6 after the drop (4,000 each). Expected 24,000 tokens, $0.072 at the Sonnet 4.6 input rate; `tokler stats --json` reports exactly that, sessions_count 8.

#### Adversarial second-pass review (the tier math earned it)

A dedicated review agent was pointed at the math with orders to break it. It did, twice:

- CRITICAL, fixed: dedup was scoped per file. Claude Code session resume copies earlier transcript records into a new jsonl with the same (message.id, requestId), so a resumed session would have double counted "ground truth". Fix: two-stage ingestion (per-file last-wins dedup, cacheable; then a global cross-file merge with a deterministic winner rule: later ts, then larger output_tokens, then lexicographically first file). Cache format bumped to v2 (v1 caches self-heal by reparse). Zero cross-file duplicates exist in this machine's logs today, so the totals did not move, but the resume scenario is now correct by construction and pinned by 5 tests.
- MAJOR, fixed: assumed-mode realized intervals did not clamp to now, so a future-dated snapshot (clock skew, copied ledger) fabricated sessions that had not happened. Both interval bounds now clamp to now; regression test pins the reviewer's exact scenario (was 20,000 phantom tokens, now 0).
- Minor, fixed: saturating arithmetic on hand-editable ledger values; codex rollouts with usage but no cwd now count as skipped instead of vanishing; stats no longer reports ingestion enabled when no home dir resolves; the scan-supersedes-mcp staleness consequence is now stated in the rustdoc.
- Verified clean by the reviewer: synthetic-after-dedup ordering, unsorted/equal-ts ledgers, global summation, cost math against the pricing table (opus-4.7 hand-checked), model-id normalization edge cases (gpt-5.4 and gemini-2.5-pro untouched by the dot conversion), cache invalidation, negative-realized rendering, and a recompute of the hand-computed test goldens.

#### Verification

Gates: CLI 118 unit + 4 integration (+2 ignored real-log probes), core 83, clippy + fmt + cargo-deny clean both workspaces, web all four gates pass, dash/egress/storage scans clean, bun pm untrusted 0. Bumped 0.4.0 -> 0.5.0.

### 2026-06-10: Phase I3, Ratatui dashboard + Windows distribution leg; 0.6.0

Two parallel agents (TUI; windows packaging) delivered IMPROVEMENTS phase I3 on top of the freshly shipped 0.5.0.

#### The dashboard (the cool part, scoped to ship)

- Bare `tokler` in a TTY opens a Ratatui dashboard (ratatui 0.29 + crossterm 0.28, the stack proven by the portfolio CLI; patterns copied, not code). Non-TTY bare invocations keep the script contract: usage to stderr, exit 2 (integration-tested: piped bare `tokler` exits fast, never hangs). `tokler stats --tui` is the explicit form; `tokler stats --compact` renders one static frame to stdout via TestBackend (works non-TTY, the screenshot artifact).
- Three tabs, deliberately small per the locked decision. Project (default): live load-profile bars for the cwd repo (a background std::thread runs the project walker; "scanning..." until ready; ledger snapshot as fallback), heaviest agent-context files, identified savings, realized sparkline over this project's snapshots. Machine: projects ranked by standing always-loaded weight, global identified + realized. Spend (only when ingestion is consented): last-30-day input-side bars from real session logs, top-model table with costs (unpriced models labeled), cache hit rate, PRICES_OBSERVED date. Footer everywhere: key hints, tier labels, and "input savings, output may vary."
- Event loop: synchronous, crossterm poll with ~1s tick; Tab/arrows, r refresh (re-reads ledger and logs, restarts the scan), q/Esc/Ctrl-C quit. Alternate screen and raw mode restored on every exit path including panic (hook). No async runtime, no new deps beyond ratatui/crossterm.
- Data plumbing: a shared StatsSnapshot (src/commands/stats_data.rs) feeds both `stats` and the TUI so the two surfaces can never disagree.

Compact frame against this repo's real data (live scan: always 6,976 / on-invocation 27,935 / docs 1,484 tokens; the heaviest file is a 6,779-token skill):

```
tokler  Project  Machine  Spend
cwd: /Users/agnel/Documents/agnel-website
+ Load profile (always, on-invocation, on-demand, docs) -----------------------+
|always         ##################....................................  6,976  |
|on-invocation  ######################################################  27,935 |
|on-demand      ......................................................  0      |
|docs           ####..................................................  1,484  |
+ Heaviest agent-context files: .agents/skills/turborepo/SKILL.md 6,779 tok ...
+ Savings (this project): identified (advisory estimate) ~85-2,054 tokens
input savings, output may vary. tiers: identified (advisory), realized ...
```

Terminal smoke, honestly stated: the interactive path was driven through a real pty (`script`): raw-mode enter and exit clean, q quits, exit 0, terminal restored; the pty reports no window size so frames render empty there, and frame content is instead proven by the `--compact` capture above plus a TestBackend buffer test. A human pass on a real terminal is the remaining nicety, not a blocker; empty-ledger and ingestion-off states render friendly setup panels (tested).

#### Windows distribution leg

- `tokler-publish.yml` gains a windows-2022 matrix leg (x86_64-pc-windows-msvc) and the publish job stages `npm/tokler-win32-x64/bin/tokler.exe`. New `tokler-win32-x64` npm package (os win32, cpu x64); the wrapper pins it in optionalDependencies; the launcher resolves `tokler.exe` via a platform-conditional binary name in both dev-path and require.resolve branches. Bump script carries the new package (round-tripped 0.5.0 -> 9.9.9 -> 0.5.0, all carriers agreeing).
- First-publish gap, planned for: the win32 publish step is continue-on-error and verify is warning-only for it, exactly the linux 0.3.0 pattern. Owner follow-up (one-time): after the merge run fails the win32 publish, download the windows artifact from that run, stage into npm/tokler-win32-x64/bin/tokler.exe, `npm publish` it manually once, then configure its Trusted Publisher on npmjs.com (repo agnelnieves/agnelweb, workflow tokler-publish.yml). Future releases then flow automatically.

#### Verification

Gates: CLI 130 unit + 8 integration tests (+2 ignored probes), core 83, clippy + fmt + cargo-deny clean (ratatui/crossterm tree license-vetted), web all four gates pass, dash/egress/storage scans clean, bun pm untrusted 0, bump script syntax + round trip verified. Bumped 0.5.0 -> 0.6.0.

### 2026-06-10: 0.6.0 shipped; windows binary built in CI, npm first-publish pending

Merge to main fired tokler-publish run 27295290899: all five build legs green including the new x86_64-pc-windows-msvc leg (the windows binary builds and the run's artifact carries tokler.exe). tokler-cli@0.6.0 plus both darwin and both linux packages published via trusted publishing. tokler-win32-x64 hit the expected first-publish gap (continue-on-error by design). Owner follow-up, one time: download the windows artifact from run 27295290899, stage it as npm/tokler-win32-x64/bin/tokler.exe, npm publish it manually, then configure its Trusted Publisher (repo agnelnieves/agnelweb, workflow tokler-publish.yml). Until then windows installs of the 0.6.0 wrapper print the platform-package-missing message (windows was never supported before, so nothing regressed).

### 2026-06-10: Phase I4, benchmark harness + /bench results page; 0.7.0

Three parallel agents (harness, methodology, web page) plus orchestrator verification delivered IMPROVEMENTS phase I4: the public, reproducible benchmark that competes on rigor instead of a headline percent.

#### Harness (apps/tokler-cli/benchmarks/, the location decision)

- Original fixtures per track (nothing copied from external sources): structural (pretty JSON config, HTML marketing page, duplicated-paragraph prompt, repeated stack-trace dumps), configuration (heavy 4-server mcpServers config, VS Code shape, an already-slimmed config), lossy (three prose docs).
- Bun runner shelling the release tokler binary: every token count comes from `tokler count --json` (o200k_base, named per case). Structural transforms run in the runner (JSON minify, HTML to Markdown mirroring the core rules, exact paragraph and frame dedup) with injection_overhead_tokens explicitly zero and variance over 3 runs asserted zero (the harness fails loudly otherwise). Configuration cases come from `tokler mcp --json` (cold, swap, slim, percent of a 200K window). Lossy runs LLMLingua-2 (the TinyBERT model the web preview uses, fetched once from the HF hub and cached; disclosed as dev tooling, the CLI itself stays no-network) at target ratios 0.7 / 0.5 / 0.33 with achieved ratios published.
- Headline numbers this run: structural 29 to 51 percent (HTML to Markdown 1,111 -> 539 = 51.5 percent; minify 32.4; dedup 44.4 and 28.7), configuration heavy config 86,250 cold tokens = 43.1 percent of a 200K window (swap reclaims 69,000, slim 35,750), lossy achieved 30-34 / 49-52 / 66-69 percent at the three targets, quality unscored and labeled.
- External comparisons, honestly: caveman-shrink (MIT) is a pure function, vendored verbatim under benchmarks/external/ with its license (root biome.json ignores that dir); measured on the lossy fixtures at 11.7 percent; on the configuration track it is marked not-comparable (it shrinks tool descriptions in flight, tokler measures static config cold cost). wilpel/caveman-compression (MIT) needs a Python venv plus a spaCy model, marked not-runnable-headless with that reason. No fabricated numbers anywhere.
- Determinism contract automated: `bun apps/tokler-cli/benchmarks/run.ts --check-determinism` runs twice and verifies the only difference is generated_at (verified again by the orchestrator post-integration). results.json (schema in apps/tokler-web/src/types/bench.ts, v1) plus generated RESULTS.md (methodology preamble + per-track tables + regeneration footer); the runner also syncs results.json into apps/tokler-web/src/data/ for the page.
- .github/workflows/tokler-bench.yml: workflow_dispatch, rust + bun setup, HF model cached, uploads results.json + RESULTS.md as artifacts.

#### Methodology (benchmarks/methodology.md, single source of truth)

840 words shipping verbatim in RESULTS.md and on /bench: why one headline percent is how the space lies to itself (the 65-percent-claim vs 9-21-percent independent reproduction story, told as baselines not villains), the seven rules (declared baselines, named tokenizers with the cl100k proxy never in headline rows, injection overhead charged, N runs with variance, date-stamped pricing, everything in the repo, input-token bounded with the arXiv 2603.23525 RCT numbers), the three track contracts, what the benchmark does NOT claim, and a reproduction recipe with the determinism diff check. No private-repo references.

#### /bench page (tokler-web)

Static server component importing the synced results.json (typed by bench.ts; no fetch, no browser storage; the privacy scans stay clean). Renders the meta line (generated_at, version, prices observed, runs), the methodology (dependency-free markdown rendering), three track tables with fidelity labels, external comparisons including the not-runnable ones with reasons, the RCT caveat leading the lossy section, and the input-token-bounded footer. Metadata: title, 143-char description, canonical /bench, OpenGraph + Twitter. Verified by SSR curl: real numbers render (86,250; 51.5 percent; the caveman-shrink rows).

#### Verification

Determinism check green post-integration. Web gates all pass; /bench builds static. Dash scans clean across fixtures, runner, page, workflow (vendored external/ exempt by design, like the drift fixtures). Privacy scans clean (one comment reworded so the literal-string gate stays meaningful). CLI 130 unit + 8 integration, core 83. bun pm untrusted 0. Bumped 0.6.0 -> 0.7.0.

### 2026-06-10: Phase I5, distribution layer; 0.8.0

Two parallel agents (report command; public-repo staging) delivered the final IMPROVEMENTS phase. The engagement's definition of done is met; the remaining items are owner-account actions, listed at the end.

#### tokler report --html (the stakeholder artifact)

- `tokler report [--global] [--html] [--output PATH]` renders a fully self-contained static HTML report: no JavaScript, no external requests, inline CSS, system fonts, print-friendly (page-break rules, A4-sane). Default output ./tokler-report.html; prints the absolute path.
- Content mirrors `stats` by construction (both consume the shared StatsSnapshot): header with scope, generated UTC, version, prices observed; one card per tier with its label rendered prominently (negative realized shown honestly as growth); measured card only when ingestion is consented (sessions, token splits, cache hit rate, priced cost, unpriced models listed by name); a 30-day pure-CSS spend bar chart; the project load profile in project scope; methodology footnotes (tier definitions, the realized formula in one quotable sentence, the input-token-bounded line, the assumed-rate disclosure, prices-observed on every dollar figure).
- Every interpolated value is HTML-escaped (tested against hostile paths). Reading is allowed under CI=true (the env kill governs writes and ingestion, not reads), matching stats. One deliberate exception to file ownership: src/tui/data.rs went pub so the report reuses the TUI's 30-day merge instead of duplicating it.
- Generated against this machine's real data and delivered to the owner: 169 sessions, 97.4 percent cache hit rate, $1,357.22 priced cost, two unpriced models named, the 30-day trend.

#### distribution/ (the staged public companion repo)

Everything the future public repo needs, prepared in-tree; the owner pushes it to a new public repository when ready (placeholders <public-repo> mark the three README spots needing the real slug). Conventions verified against current docs, not memory; one correction surfaced: marketplace.json lives INSIDE .claude-plugin/, not at the repo root.

- skills/tokler-audit, tokler-slim, tokler-optimize: SKILL.md files (frontmatter names match dirs, verified) teaching agents to run `npx tokler-cli@latest <cmd> --json`, parse the real emitted fields, act, RE-RUN to verify the delta, and always report savings with tier labels and the input-token caveat. tokler-slim closes the realized-savings loop (apply slim snippet, re-measure, report); tokler-optimize never applies lossy rewrites without explicit user opt-in.
- .claude-plugin/plugin.json + marketplace.json: the same repo doubles as a Claude Code plugin (name tokler, 0.7.0 at staging time).
- action/: composite GitHub Action generalizing tokler-audit.yml for any repo. Inputs: fail-on (none default), working-directory, comment-mode (sticky default), version (pins tokler-cli). On pull_request it upserts ONE sticky comment (marker <!-- tokler-report -->, requires permissions pull-requests: write, documented); on other events it writes the same markdown to the step summary. Dependency-free node script builds the report from `tokler project --json`; a TOKLER_BIN override exists for local/dev runs.
- README.md (public-facing, zero private references, scanned) with the three install paths and the support matrix; MIT LICENSE.
- .github/workflows/tokler-action-dryrun.yml in THIS repo: workflow_dispatch harness running the local action against this repo with comment-mode off; proves the composite end to end in CI before any public repo exists.

#### Verification

CLI 135 unit + 10 integration tests (+2 ignored probes), clippy/fmt/deny clean, core 83, web gates pass, dash scans clean across distribution/ and the new sources, private-reference scan over distribution/ clean (author name only), JSON manifests parse, skill frontmatter validated. Bumped 0.7.0 -> 0.8.0.

#### Owner actions outstanding (the complete list)

1. tokler-win32-x64 one-time manual publish (artifact from publish run 27295290899 or any newer run), then its Trusted Publisher config.
2. Create the public companion repo; push distribution/ as its root; replace the three <public-repo> placeholders; tag v1 for the action. Dispatch tokler-action-dryrun in this repo first if extra confidence is wanted.
3. ANTHROPIC_API_KEY repo secret for the weekly drift watchdog (carried over).
4. A human pass over the TUI in a real terminal (headless smoke proved enter/exit/keys; the frames are proven by --compact and tests).

### 2026-06-10: 0.8.0 shipped; action dry-run green; engagement complete

The I5 merge published tokler-cli@0.8.0 with both darwin and both linux packages via trusted publishing (run 27298076923); tokler-win32-x64 remains the one-time manual publish. The tokler-action-dryrun workflow ran on main (run 27298094428) and succeeded: the composite action in distribution/ executes end to end in CI, builds the report from `tokler project --json`, and writes the step summary. Phases I1 through I5 are all merged, published, and logged above; the outstanding items are the four owner actions listed at the end of the I5 entry.

### 2026-06-10: Renamed tokler -> tolkin; 0.9.0

Owner branding decision, executed at the cheapest possible moment: before the public companion repo, before the blog post, before tokler-win32-x64 was ever published. All six tolkin npm names were verified available; the Tolkien-trademark adjacency was flagged explicitly and accepted by the owner.

#### What changed

- Everything forward-looking: directories (packages/tolkin-core, apps/tolkin-cli, apps/tolkin-web), crate names (tolkin-core, tolkin-core-wasm, binary `tolkin`), the six workflows (tolkin-*.yml), all six npm package stagings (tolkin-cli wrapper + five platform packages), the distribution/ staging (skills tolkin-audit/slim/optimize, plugin "tolkin", action, README), the web app and /bench page, the benchmark harness (results regenerated under the 0.9.0 binary, determinism contract re-verified), and the operating docs. 182 paths moved with git mv; 103 files content-renamed.
- History stays true: the Log entries above, LESSONS.md entries, HANDOFF.md (banner added), IMPROVEMENTS.md, and EXECUTION-PROMPT.md keep their original names. The Status and Workspace tables describe current state and were renamed.

#### Compatibility (a rename must not eat anyone's data)

- Env vars: TOLKIN_DATA_DIR / TOLKIN_NO_LEDGER, with the TOKLER_* names still honored as fallbacks.
- Data dir: on first run, an existing pre-rename "tokler" data dir is silently moved to "tolkin" (fs::rename; failure means a fresh start, old data left in place).
- Ledger schema: records now write v2 with `tolkin_version`; the reader accepts pre-rename records via a serde alias for `tokler_version` (verified against a seeded old-format ledger: parses, zero skipped).
- The bench harness also gained a CI fix discovered by the post-Node-24 dispatch: bun's isolated linker in CI does not hoist the web app's LLMLingua-2 dependency to the root, so the harness now resolves it from the web workspace's context when the bare specifier misses.

#### The npm cutover (owner actions, the complete list)

npm packages cannot be renamed; the cutover is new names plus deprecation:

1. The first tolkin-publish run on main builds all five binaries and attempts all six publishes in transition mode (every step continue-on-error, verify warning-only). All six will fail OIDC first-publish, by design.
2. Manually first-publish each package once from the staged npm/ dirs (binaries from that run's artifacts): tolkin-darwin-arm64, tolkin-darwin-x64, tolkin-linux-x64, tolkin-linux-arm64, tolkin-win32-x64, then tolkin-cli last.
3. Configure Trusted Publishers for all six on npmjs.com: repository agnelnieves/agnelweb, workflow filename tolkin-publish.yml (note: the FILENAME changed; old tokler trusted-publisher entries do not carry over).
4. Deprecate the five live tokler packages: `npm deprecate tokler-cli "renamed to tolkin-cli"` (same for tokler-darwin-arm64, tokler-darwin-x64, tokler-linux-x64, tokler-linux-arm64). Do not unpublish; deprecation keeps existing installs working.
5. Tell me when the six first publishes are done and I re-tighten the publish workflow (wrapper + darwin strict again).
6. tokler-win32-x64 never ships; tolkin-win32-x64 takes its place in the matrix.

#### Verification

CLI 135 unit + 10 integration tests, core 83, clippy + fmt + cargo-deny clean both workspaces, wasm pkg emits tolkin-core-wasm, lockfile regenerated, web gates all pass, bench determinism holds under the renamed binary, fresh onboarding smoke greets "Welcome to tolkin", old-env-var fallback verified, pre-rename ledger record parses. Bumped 0.8.0 -> 0.9.0.

### 2026-06-10: Rename merged; bench harness proven end to end in CI

tolkin-ci green on the rename (one rerun for a Docker Hub flake on the cargo-deny action, infrastructure not code). The transition-mode publish run (27300625236) built all five binaries and staged them as artifacts; all six publish steps failed softly on the OIDC first-publish gap exactly as designed. The action dry-run passed. The bench harness needed three environment fixes to go green on a CI runner, each found by an actual dispatch: bun's isolated linker does not hoist the web app's dependencies to the root (resolve @atjsh/llmlingua-2, @huggingface/transformers, and gpt-tokenizer from the web workspace's context), and transformers.js device "auto" requests the CUDA execution provider on linux, whose shared library does not exist on GPU-less runners (pinned to cpu; results byte-identical, macOS was already cpu). tolkin-bench now runs green on main.

Note for the owner: until the six tolkin first publishes land, the version gate cannot find tolkin-cli on the registry, so every push to main re-runs the full publish matrix (fail-soft, ~15 minutes of CI each). The cutover ends that.

### 2026-06-10: npm cutover complete; tolkin live on the registry

All six tolkin packages first-published manually (OTP relay through the session): tolkin-darwin-arm64, tolkin-darwin-x64, tolkin-linux-x64, tolkin-linux-arm64, tolkin-win32-x64, and the tolkin-cli wrapper, all at 0.9.0. Windows ships for the first time in the project's history (tokler-win32-x64 never existed; tolkin-win32-x64 does). All five tokler packages deprecated with "renamed to tolkin-*" pointers; nothing unpublished, existing installs keep working. Clean-room verification: `npx tolkin-cli@latest --version` prints tolkin 0.9.0 from the live registry and `count - --all` returns the three-provider table.

Publish workflow re-tightened out of transition mode: wrapper and darwin strict, linux and win32 warning-only until their trusted publishers are confirmed. Owner follow-ups now down to: configure six Trusted Publishers on npmjs.com (repo agnelnieves/agnelweb, workflow tolkin-publish.yml) and say the word so the remaining legs go strict; create the public repo from distribution/; ANTHROPIC_API_KEY secret; one human TUI pass.

### 2026-06-10: Wave 0 review hotfixes (P0-1, P0-2, P1-1 through P1-5); 0.9.1

First wave of the I6 engagement executing REVIEW-FINDINGS.md. Five fixes plus one research memo, run as four parallel worktree agents, one serial agent, and two orchestrator-inline fixes. Trusted publishers were confirmed before this wave (commit afe74aa), so this is the first version that publishes all six packages strictly with zero owner action.

#### P0-1: tolkin-audit.yml un-parse-killed (orchestrator, inline)

The two top-level `env:` mappings (introduced by the rename commit 168fe98) are merged into one carrying both `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24` and `TOLKIN_FAIL_ON`. Verified: every tolkin workflow now has exactly one `env:` block and the file YAML-parses. Done inline rather than by agent: a two-line fix in a file already fully read. Live proof on a real PR follows the merge (entry below).

#### Research memo (A-R1): three facts pinned with primary sources

- Gemini 2.5 family cached-token pricing IS published, at 10 percent of base input: Pro $0.125/MTok (under 200K; $0.25 over), Flash $0.03, Flash-Lite $0.01, storage fee for explicit caching (Pro $4.50, others $1.00 per MTok per hour), implicit caching automatic on the family (ai.google.dev/gemini-api/docs/pricing, /docs/caching, read 2026-06-10).
- The Guzik caveman benchmark (dev.to/jakguzik/i-benchmarked-the-viral-caveman-prompt-to-save-llm-tokens-then-my-6-line-version-beat-it-2o81) supports 9-21 percent as the full-study range (Opus full-prompt 9, Opus micro-prompt 21, Sonnet 13-14; plain no-brevity baseline; output-token metric). The researches' 14-21 percent is the micro-prompt slice. methodology.md's existing 9-21 citation was already correct; the source URL and the output-side precision were added.
- Arcade.dev measured Anthropic Tool Search regex-variant retrieval at 56 percent (14/25 tasks, 4,027-tool corpus; BM25 64 percent) at arcade.dev/blog/anthropic-tool-search-4000-tools-test. Banked for the Tool-Search caveat task in a later wave.

#### P0-2: cache discounts now derive from the pricing table (A2, opus + adversary-grade tests)

`mcp::scenarios()` no longer hardcodes warm multipliers. A `cache_multipliers(provider)` helper derives cold (cache_write_5m over input, 1.0 when None) and warm (cache_read over input, 1.0 when None) from `pricing::default_for(provider)`, so the analyzer and the cost calculator can never disagree again. Effect: OpenAI warm 0.50 -> 0.10 (matching the GPT-5 family cache_read), Gemini warm 0.25 -> 0.10, Anthropic unchanged (1.25 cold, 0.10 warm). The Gemini rows in pricing.rs now carry the published cached rates above (the long-context tier's cached rate and the storage fee are documented as not modeled). Two-confirmation evidence: unit tests compute every expected multiplier from the pricing table at runtime (never hardcoded), and live release-binary smokes show warm = cold_tokens x 0.10 for all three providers plus `tolkin cost --model gemini-2.5-pro --cache-hit-rate 1` billing cached input at exactly $0.125/MTok with cost.rs untouched.

#### P1-5: the GitHub report no longer disagrees with itself (A2 decision, option b)

Kept the generic Tool Search formula (500 stub + min(tools,5) x 600; for GitHub 3,500) and removed the contradicting 8.7K claim from the catalog note. Reasoning recorded: the formula is uniform across the catalog and maps to how the Tool Search Tool works; scaling per-server from cold_tokens would propagate estimate uncertainty while adding no information; the 8.7K launch-era figure traces to the vendor-reported 85 percent reduction claim, so it belongs in attributed prose, not next to a computed column. A regression test pins the note against contradiction. Orchestrator correction on top: A2 had attributed the 26-55K range and the 8.7K figure to Scalekit; Scalekit actually published a single GitHub measurement (44,026 tokens) and the 65x Linear figure. The note now labels 26-55K an externally reported multi-source range and PLAN section 9 attributes 8.7K to the vendor claim.

#### P1-1: skills regenerated from live contracts; drift now impossible (A3)

The fabricated rule ids (`oversized-skill-body`, `shell-export-secret`) are gone; the audit skill names real, commonly-firing rules (the product ships 13: near-duplicate-paragraphs, json-verbosity, stack-trace-verbosity, volatile-prefix, sub-cache-threshold, html-content, filler-phrases, plus 6 experimental) and routes secrets guidance at the real `secret_files` field. The optimize skill's entirely fabricated `stats --json` schema was replaced with the real shape (scope, project_key, generated_at, prices_observed, realized_rate, ledger, ingestion, tiers) from a live seeded run. The slim skill's scan/mcp blocks gained the real keys. The cross-file near-duplicate overclaim is corrected to per-file truth. New gate: `apps/tolkin-cli/scripts/check-skill-schemas.ts` parses every `<!-- tolkin-schema: <cmd> -->`-annotated JSON block in the skills, runs the built binary in a seeded temp environment (temp HOME with synthetic logs, temp TOLKIN_DATA_DIR, both consents), and fails if any documented key is absent from live output or any skill version disagrees with Cargo.toml. Wired into tolkin-ci after cargo test (debug binary). Proven by reintroducing a fake key (lint fails naming it) and restoring (7/7 checks green). bump-version.sh now carries the three skill versions (round-tripped 0.9.0 -> 9.9.9 -> 0.9.0, all nine carriers agreeing).

#### P1-2 + P1-3: distribution and benchmark truth (A4)

README: Windows x64 row says Live (tolkin-win32-x64@0.9.0 is on the registry), the scan caption describes scan, and the benchmarks pointer resolves: `distribution/benchmarks/RESULTS.md` is now a generated mirror, synced by the bench runner on every run so it cannot go stale (orchestrator trimmed a dangling "/bench page" phrase; no public URL exists yet to anchor it). The action's setup-node is on Node 24. Track 2 honesty: methodology.md now states the configuration numbers are representative catalog estimates (not tokenized manifests), the runner writes "catalog estimate" instead of a tokenizer attribution on those rows, and the /bench page header, section copy, and metadata all carry the catalog-estimate label while structural and lossy stay measured. Artifacts regenerated; headline numbers unchanged (mcp-heavy 86,250 cold, 43.13 percent of a 200K window; HTML-to-Markdown 51.5 percent); determinism contract re-verified twice (the second time on the final merged tree, where the only artifact diff was generated_at, proving the A2 core change moved no benchmark number).

#### P1-4: the cost default is input-side (A5, opus)

When no output token count is supplied, the calculator now bills zero output, sets `output_estimated: false`, and prints "Output not included; supply an output token count to model it." The old behavior (output = input x the output:input PRICE ratio) is an explicit opt-in: `estimate_output` in the core request (serde default false, WASM-additive), `--estimate-output` on the CLI, an off-by-default toggle labeled a rough volume assumption on the web panel. Live smokes, both legs: default per-call total equals input-side cost exactly ($0.001075 for 430 gpt-5.4 tokens); opt-in reproduces the old number ($0.111185, output 97.3 percent of total) under its label. PLAN section 10 rewords the 5x/6x/8x figures as price ratios. The existing core tests that assumed the estimate were updated; new tests pin both paths, computing expectations from the pricing table.

#### Verification (combined tree, run by the orchestrator)

| Check | Result |
|---|---|
| cargo test (cli) | 136 unit + 10 integration pass (+2 ignored real-log probes) |
| cargo test (core) | 90 pass |
| clippy + fmt + cargo-deny, both workspaces | clean |
| wasm-pack release build | pass |
| web gates (lint, lint:fast, typecheck, build) | all pass |
| skill schema drift lint | 7/7 green |
| bench determinism | holds; artifacts differ only at generated_at vs committed |
| dash scan / egress scan / bun pm untrusted | 0 / 0 / 0 |
| Browser verification | SSR curl on the dev server: cost panel renders the new default note, toggle label, and "input only" placeholder; /bench renders the catalog-estimate labels and unchanged numbers. The toggle's interactive reflow was not visually exercised (no browser tooling in session); the wiring is covered by unit tests and the rendered DOM copy. |

Incident recorded in LESSONS.md: the orchestrator's shell cwd drifted into completed agent worktrees twice, landing one merge on the wrong branch (recovered by SHA-merging from the main checkout; no work lost). Bumped 0.9.0 -> 0.9.1.

### 2026-06-10: 0.9.1 shipped; audit workflow proven live on a real PR

The wave 0 merge published 0.9.1 across all six packages via trusted publishing with every leg strict (run 27311934160), the first release needing zero owner action. tolkin-ci green on main including the skill schema drift lint's first CI execution. The P0-1 proof landed: PR 26 (a real docs change adding the drift lint to CLAUDE.md/AGENTS.md) triggered the repaired tolkin-audit workflow, which ran green and posted the sticky comment with the per-file token table (both changed files at ~1K tokens, zero findings) and the repo load profile (always 7,453 / on-invocation 32,336). Registry verified: tolkin-cli@0.9.1, tolkin-darwin-arm64@0.9.1, tolkin-win32-x64@0.9.1. PR 26 merged; wave 0 closed. Wave 1 (`tolkin cache` slice 1) is in flight.

### 2026-06-10: Wave 1, tolkin cache slice 1; adversarially reviewed; 0.10.0

The owner's stated priority from REVIEW-FINDINGS (the deep-dive spec): measured prompt-cache health from the consented logs, the capability neither research realized was possible locally. One author agent (A6) built it; a separate adversarial agent (A7, orders to break it) reviewed it; the wave carried both their commits.

#### What shipped

- **Per-request retention.** The Claude Code reader retains a compact per-request tuple per session (ts, fresh input, cache read, 5m write, 1h write), populated AFTER the I2 global cross-file dedup so resumed sessions never double count (pinned, including the nastier different-usage-numbers winner case A7 added). Parse cache bumped to v3; v1/v2 self-heal by silent reparse (tested, plus corrupted-cache fallback). Codex rollouts carry no cache-write fields, so cache analysis is Claude-Code-sourced for now and every surface says so.
- **`src/cache_analysis.rs`** (pure module, 16+ golden and edge tests): hit rate per scope with the under-0.5 active-day broken-cache advisory (ground truth); write churn as tokens written after a session's first write with worst sessions named by id and start ts only (ground truth, with the disclosure below); the 5m vs 1h TTL counterfactual simulated over the real gap timeline (reads refresh the TTL, gap strictly over the TTL forces a re-write, gap equal to the TTL is a hit; all boundary-tested), priced at per-model MARGINAL write rates (write minus the 0.1x read you would have paid anyway: 1.15x and 1.9x input, derived from the pricing table and pinned by test for all four Anthropic models); break-even rendered as: the 1h TTL wins exactly when 1.9 x W1 < 1.15 x W5; cadence facts (intra-session gaps over 5m, per-project inter-session gaps under 1h, zero-cache-read sessions). Sessions priced at their dominant model; unpriced models excluded from dollars and disclosed with the priced share.
- **Surfaces:** `tolkin cache [--global] [--json]` (consent-gated like stats; stable JSON schema); an ADDITIVE `cache` block in `stats --json` measured output (injected at the serialization layer, no existing key moved; the skills drift lint stayed green, 7/7); one TUI Spend-tab row (broken-cache advisory only); one Prompt cache health section in `report --html` (same shared-snapshot pattern as stats). Labeling everywhere: observed numbers Tier 3 ground truth; every simulated number Tier 1 advisory estimate computed from Tier 3 inputs; the scope line prints always (Claude Code manages its own caching; prefix stability and session shape are the levers there; TTL choice is an API-builder lever); input-token-bounded footer.

#### The adversarial review (A7), in writing

A7 verified the wave's named risk first: cache reads DO refresh both TTLs. Anthropic platform docs ("The cache is refreshed for no additional cost each time the cached content is used", platform.claude.com/docs/en/build-with-claude/prompt-caching) state it generally; the Bedrock docs state it explicitly for the dual-TTL models ("The cache has a Time To Live (TTL), which resets with each successful cache hit", docs.aws.amazon.com/bedrock/latest/userguide/prompt-caching.html). Both quotes and URLs are pinned in the rustdoc of `simulated_write_events_reads_refresh_ttl`, with the caveat that Anthropic publishes no 1h-specific refresh sentence, so the verdict text and simulation must change together if that ever diverges.

A7's sign-off block, verbatim disposition list:

> SIGN-OFF: yes
> - BREAK Churn-1: plain CLI headline flattened the metric to "a write after the first means the prefix changed mid-session", a strong claim the CHURN_NOTE then had to walk back. FIXED: the headline now describes what the share counts; the CHURN_NOTE carries the growth-vs-instability framing. Regression pinned by `cache_plain_churn_headline_survives_the_hostile_reader_test`.
> - PIN TTL-1: refresh-on-read for 1h TTL holds (Anthropic general sentence plus Bedrock explicit statement). Citation updated from placeholder to two quoted statements with URLs. Simulation unchanged.
> - PIN Dedup-2: same (message_id, requestId) in two files with DIFFERENT cache numbers retains the WINNER's tuple. New test `cross_file_retention_keeps_winners_cache_tuple_not_losers`; no code fix needed.

Attacks survived without change (existing tests named in A7's full report): TTL boundary semantics, equal-ts ordering, out-of-order and future timestamps (saturating arithmetic), one-file-multiple-cwds, overlapping sessions, per-project gap scoping under --global, v2 self-heal, corrupted caches, tier labels and disclosures on every surface, all empty states and division-by-zero edges, HTML escaping, TUI pathological values, the stats additive contract.

#### Reconciliation (the acceptance gate, the I2 ritual)

A6's double run on its binary, `tolkin cache --global --json` vs an independent recompute (`scripts/cache-recompute.ts`, spec implemented from scratch), real logs, read-only: 176 sessions / 8,908 requests, hit rate 0.97522 (cache_read 1,776,801,009 of input-side 1,821,939,650), observed writes 0 at 5m / 44,941,903 at 1h, churn share 0.94268, simulated W5 565 events / 9,074,066 tokens, W1 255 / 3,670,734, dollars $37.48 (5m) vs $29.29 (1h), delta -$8.19, priced share 0.7795 (claude-fable-5 and claude-opus-4-6 unpriced), intra gaps over 5m 389/8,732, inter gaps under 1h 39/106, zero-cache-read sessions 2. Every field matched bit for bit, both runs. Orchestrator re-ran the pair twice on the final merged tree: the second pair matched exactly on all 24 compared fields (178 sessions / 9,012 requests by then); the first pair differed by exactly one request's worth (9,005 vs 9,006) because this very session appends to the logs between the pair's two sequential invocations, the live-log race A6 predicted. The TTL verdict on this machine's real data (advisory estimate computed from ground-truth gaps): the 1h TTL is the cheaper strategy (1.9 x W1 = 6.97M < 1.15 x W5 = 10.44M marginal write tokens), matching the TTL Claude Code actually uses (100 percent of observed writes are 1h).

#### Found along the way (queued, not relitigated)

- **Subagent-transcript ingestion gap (P1-class, pre-existing, affects all measured surfaces):** current Claude Code nests subagent transcripts at `~/.claude/projects/<slug>/<session-id>/subagents/agent-*.jsonl`, one level deeper than the I2 discovery walk reads; their usage records do not appear in the parent session file, so measured totals undercount subagent fan-out spend. Deliberately NOT fixed in this slice (it moves every measured surface and needs its own reconciliation); queued for wave 2.
- **Per-model 1h-TTL availability footnote:** Bedrock lists some current models as 5m-only while the 4.5 family carries both TTLs; the counterfactual presents the 1h strategy without a per-model availability gate. Queued as polish (wave 3 hygiene).
- **Churn field rename** (`share` reads like a defect score; a growth-neutral name would be better) is a documented-keys schema change; deferred deliberately.
- Context for the launch story: a March 2026 incident silently reverted some setups' default TTL from 1h to 5m (anthropics/claude-code#46829), evidence that wild 1h usage may be lower than logs imply. No effect on the refresh semantics this slice depends on.

#### Verification (combined tree, orchestrator-run)

| Check | Result |
|---|---|
| cargo test (cli) | 161 unit + 16 integration pass (+2 ignored real-log probes) |
| cargo test (core) | 90 pass |
| clippy + fmt + cargo-deny, both workspaces | clean |
| wasm-pack release build | pass |
| web gates (lint, lint:fast, typecheck, build) | all pass |
| skill schema drift lint | 7/7 green (the additive stats cache block broke no documented key) |
| dash scan / egress scan / bun pm untrusted | 0 / 0 / clean |
| bench determinism | not required this wave (no benchmarks/ or counting-path change); `count` smoke green |
| Reconciliation | table above; exact match |

Bumped 0.9.1 -> 0.10.0 (minor: new subcommand, new persistence format v3).

### 2026-06-11: 0.10.0 published; windows CI hotfix (TOLKIN_HOME_DIR seam)

The 0.10.0 publish went green across all six packages (run 27315174901), but tolkin-ci's windows job failed on the two new cache integration tests: they seed a synthetic log tree under a temp HOME, and `dirs::home_dir()` on Windows resolves through the known-folder API (verified in dirs-sys 0.4.1 source), which ignores HOME and USERPROFILE entirely, so the seeded logs were invisible and the hand-computed assertions saw zero sessions. Fix: usage discovery now resolves from `usage::home_root()`, which honors a `TOLKIN_HOME_DIR` override (test and dev plumbing, the TOLKIN_DATA_DIR pattern) before falling back to the real home; the cache tests set it alongside HOME. Production behavior unchanged when the variable is absent. Windows job green on the fix (run 27315415628); registry carries tolkin-cli@0.10.0. Wave 1 fully closed; wave 2 (CI delta gates, tools/list ingestion plus bench upgrade, measured advisories, and the subagent-transcript ingestion fix) dispatched as four parallel worktree agents.
