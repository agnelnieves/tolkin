> 2026-06-10: the project was renamed tokler -> tolkin (binary, crates, npm packages, workflows). This snapshot predates the rename; read names accordingly.

# Tokler: Context and Handoff

Snapshot date: 2026-06-09. Branch: `feat/tokler`.

This is a self-contained briefing for an agent (or person) picking up Tokler. It captures the original product intent and everything built so far. The living sources of truth remain:

- `apps/tokler-web/PLAN.md`: the full architecture, the detection catalog, and references. Read it before substantive work.
- `apps/tokler-web/PROGRESS.md`: the canonical, append-only work log. Update it at the end of every work unit.
- `apps/tokler-web/CLAUDE.md` and `AGENTS.md`: the operating rules (kept in sync with each other).
- Root `/CLAUDE.md`: monorepo-wide toolchain and content rules.

This document is a snapshot and index, not a third source of truth. When it conflicts with PLAN.md or PROGRESS.md, those win.

---

## 1. What Tokler is (original intent)

A privacy-first AI token analyzer that runs entirely in the browser, and also ships as a Rust CLI for SDLC pipelines and one-shot `bunx` / `npx` workflows. It analyzes prompts, configs, MCP setups, and docs across the three frontier providers (OpenAI, Anthropic Claude, Google Gemini) and recommends savings with quantified dollar impact, confidence, and citations.

The driving mandate (from the original user context): save as much token consumption as possible to cut inference and token costs in an AI agent / SDLC pipeline at work. At roughly 15 PRs per engineer per week, agent setups have been measured at about 400K input tokens per PR, with re-sent context accounting for a large share of the bill. That is where the biggest wins live.

Name: "Tokler" (the bare "Tokenist" name collided with an established crypto/fintech site at tokenist.com). Tagline candidates: "Audit your prompts. Nothing leaves your browser." or "See every token before you spend a dollar."

### Locked-in product decisions

1. Frontier providers only for v1 (OpenAI, Anthropic, Google).
2. Analysis and recommendation only, no transformation in v1. Phase 3 may add actual compression preview (LLMLingua-2 in-browser).
3. Privacy-first, but an opt-in hybrid lever is acceptable when accuracy demands it. Hybrid is opt-in per session, with the user pasting their own API key into local memory only.
4. Tool surfaces: tokenization visualizer, context optimizer with multiple suggested approaches, MCP-config-to-CLI recommender, secret stripping on uploaded configs.
5. Frictionless distribution: also runs via CLI / `npx` / `bunx`.
6. Open source potential later; for now it lives inside the personal-website monorepo.

### Non-negotiable product principles

- Honesty is the trust moat. Every estimate is labeled as such.
- Never render fabricated per-token visualization for Claude (its tokenizer is closed). Show count plus a confidence band only.
- All savings claims are input-token-bounded. Surface "input savings, output may vary" everywhere a number appears. (A pre-registered RCT found aggressive compression can increase total cost because output grows, so input-bounded is the honest claim.)
- Nothing leaves the browser. No telemetry, no remote logging.

---

## 2. Where it lives, and the binding conventions

Monorepo root: `/Users/agnel/Documents/agnel-website` (Turborepo). The Tokler work is on branch `feat/tokler`.

The GitHub repo is PRIVATE (`github.com:agnelnieves/agnelweb`). Never link to it from any public-facing content. When referring to code generically in public docs, describe it without the repo URL.

### Three workspaces

```
packages/tokler-core/      Rust crate + WASM artifact. The single source of truth
                            for non-tokenization logic.
apps/tokler-cli/           Rust binary named `tokler`. Same core logic via CLI.
apps/tokler-web/           Next.js 16 browser UI.
```

### Toolchain (Rust-first; do not regress)

- Package manager: bun. Never `npm install` / `pnpm install` / `yarn install`. Use `bun add`, `bun install --frozen-lockfile`, `bunx`. The only lockfile is `bun.lock` (text). `bunfig.toml` pins `exact = true`.
- Lint / format: Biome 2 (lint + format + import sort) plus oxlint as a CI speed gate. No Prettier, no ESLint.
- Type checking: tsgo (TypeScript 7 Go-based preview).
- Bundler: Turbopack (never add a `--webpack` flag).
- CSS: Tailwind v4 with Lightning CSS.
- Rust: `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, `cargo-deny` (license allow-list in each workspace's `deny.toml`). Release profile: `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
- WASM: `wasm-pack build --target web` produces `pkg/`, consumed by the web app as the `tokler-core-wasm` workspace dependency.
- Supply chain: bun blocks postinstall scripts by default. Do not add to `trustedDependencies` without a real reason. Run `bun pm untrusted` after every `bun add`.

### Content rules (strict)

- NO em-dashes or en-dashes in any human-written content: blog posts, READMEs, comments, commit messages, UI copy. Use periods, commas, parentheses, colons, or sentence breaks. Hyphens inside compound words and identifiers are fine. (Em-dashes are a recognizable AI-generated-text tell; the whole project reads as written, not generated.)
- NO `Co-Authored-By` lines or any AI attribution / watermark in commit messages.
- Never log any work from this project into basestream.

### Privacy posture (enforced)

- No localStorage, no IndexedDB, no Service Worker cache by default. No `fetch` egress of user content. Saved scenarios (future) would be opt-in IndexedDB behind an explicit consent screen.
- Hybrid verification (Anthropic `count_tokens`, Gemini `countTokens`) is opt-in per session; the user supplies their own key into local memory only; only redacted text is ever sent.

---

## 3. Architecture

```
                       packages/tokler-core (Rust)
                       rules + MCP analyzer + cost + redactor + pricing
                              |                     |
                wasm-bindgen  |                     |  rlib (native)
                              v                     v
                    tokler-core-wasm        apps/tokler-cli
                    (pkg/, npm-style)        (clap binary `tokler`)
                              |
                       TS import via Web Worker
                              v
                       apps/tokler-web (Next.js 16)
```

Why Rust core + WASM + native, not pure TS or pure Rust: the high-value logic (regex over megabytes of paste, MinHash, MCP tool tokenization, cost math, pricing tables) belongs in Rust, with one regression-test suite serving both surfaces. Tokenization stays per-platform because the BPE / SentencePiece libraries are mature, fast, and large; reinventing them for cross-platform parity is poor ROI.

### Tokenization is platform-native (the core never tokenizes)

| Provider | Web | CLI | Accuracy |
|---|---|---|---|
| OpenAI | `gpt-tokenizer` o200k_base (in a Web Worker) | `tiktoken-rs` o200k_base | Exact |
| Gemini | `@huggingface/transformers` Gemma SPM | `tokenizers` crate + bundled Gemma JSON | Exact |
| Claude | `gpt-tokenizer` cl100k_base subpath | `tiktoken-rs` cl100k_base | Labeled estimate, about +/-10% |

Claude has no public tokenizer. cl100k_base is the documented proxy (it is exactly what the community `bpe-lite` library uses under the hood for its `anthropic` provider). Exact Claude counts are deferred to the Phase 2b opt-in hybrid `count_tokens` call.

### Key structural facts

- There is NO root Cargo workspace. Each Rust crate is independent (mirrors the existing `apps/cli` portfolio CLI). `packages/tokler-core` is its own 2-crate Cargo workspace: `tokler-core` (rlib) and `tokler-core-wasm` (cdylib). `apps/tokler-cli` path-depends on `tokler-core`.
- The web app loads two dedicated Web Workers: one for tokenization (owns the Gemma model, loaded at most once) and one for the tokler-core WASM (redact / cost / models / analyze_mcp). Workers keep the heavy work and the raw pasted text off the main thread.
- `pkg/` (the wasm-pack output) is gitignored and rebuilt by the turbo pipeline. A clean checkout builds it; do not commit it.

---

## 4. What has been built (phase by phase)

All phases below are complete and committed on `feat/tokler`. Phases 1c and 2a were additionally verified in a real browser.

### Phase 0: Foundation (commit 2e3e345)
Scaffolded all three workspaces with Cargo workspaces, Turbo tasks, Biome / oxlint / tsgo. Empty WASM binding (`version()`) consumed by the web app to prove the integration.

### Phase 1a: Multi-provider tokenization (commit ecd80d7)
Token counting across OpenAI (exact), Gemini (exact via Gemma), and Claude (cl100k_base proxy, labeled). Live in CLI (`tokler count`, `tokler compare`) and web (live count panel). CLI embeds `Xenova/gemma2-tokenizer` (about 17.5 MB tokenizer.json) via `include_bytes!`; web lazy-loads `Xenova/gemma-tokenizer` from the HF Hub on first Gemini use.

### Phase 1b: File ingestion + visualizer (commit 68171c2)
- File ingestion: PDF (`pdf-extract` in Rust, `pdfjs-dist` in web), DOCX (`zip` + `quick-xml` in Rust, `mammoth` in web), XLSX (`calamine` in Rust, SheetJS CE in web). Text and code formats pass through. CLI uses `parse::extract` (extension sniff, then magic bytes via `infer`, then UTF-8 text).
- Tokenization visualizer: colored per-token chips for OpenAI and Gemini (with byte-offset segments and visible whitespace), count-only confidence band for Claude (no fabricated chips).

### Phase 1c: Pricing + cost calculator + secret redaction (commit 774b856, browser-verified)
The Rust core gained three modules, exposed to the CLI natively and the web via WASM.
- `pricing.rs`: vendored mid-2026 tables, 11 models, with cache-read / 5m and 1h cache-write rates and the Gemini 200K long-context cliff.
- `cost.rs`: `estimate(CostRequest) -> CostBreakdown`. Models fresh vs cached input, cache-write, output (estimated from the provider output:input ratio unless supplied), the batch 50% discount, the long-context cliff, and a calls multiplier. Input-token-bounded with honesty notes.
- `redact.rs`: always-on secret redactor. Vendor catalog (OpenAI, Anthropic, Google AI, AWS, GitHub, Slack, Stripe, JWT, private keys, DB URIs, Notion, Linear, Figma, Neon, Supabase) plus structural rules (config `key: value`, `--flag value`, Authorization headers) and a generic high-entropy pass. Confidence = 0.4 base + 0.3 vendor-regex + 0.2 keyword-context + 0.1 entropy; at or above 0.5 auto-redacts, below goes to a review ledger. Deterministic `<REDACTED:kind>` placeholders (no length leak). `--strict` raises the threshold to 0.7 (fewer, higher-precision redactions).
- CLI: `tokler redact`, `tokler cost`.
- Web: redaction runs first and feeds the count panel, visualizer, and a new cost calculator; a redaction ledger shows findings without exposing secret bytes.

### Phase 2a: MCP analyzer, the wedge (commit 2c9eacb, browser-verified)
The headline feature. Paste any agent's MCP config and get the token cost of its tool definitions plus recommendations to swap servers for their official CLI. Client-agnostic.
- `mcp.rs`: `analyze(config_text, provider) -> McpAnalysis`. A string-aware JSONC sanitizer (strips `//` and `/* */` comments and trailing commas) feeds serde_json. Auto-detects the client shape: `mcpServers` (Claude Desktop / Claude Code / Cursor / Continue), `mcp.servers` (VS Code settings), `servers` (VS Code / Copilot), `context_servers` (Zed, including its `{ path, args }` command-object form). Handles stdio and URL / SSE transports.
- A curated 22-server catalog (GitHub, GitLab, git, filesystem, Postgres, Neon, Supabase, Notion, Linear, Slack, Jira, AWS, Kubernetes, Xcode, Figma, Sentry, Brave, Playwright, Puppeteer, fetch, memory, Drive). Each entry has a representative cold-cache token cost, an official CLI alternative (where one exists), a replace / replace-for-ad-hoc / keep recommendation, and the reasoning. Matched by command / args / name substrings.
- Per-server scenarios (cold with the provider cache surcharge, warm with the cache discount, a Tool-Search defer-loading stub, raw capacity) and totals (percent of a 200K window, total reclaimable tokens, dollar savings at the provider input rate). Unknown servers are flagged and excluded from totals.
- CLI: `tokler mcp`. Web: a standalone MCP analyzer panel.

---

## 5. Current surface (what works today)

### CLI (`apps/tokler-cli`, binary `tokler`, about 25.5 MB)

Implemented: `count`, `compare`, `viz`, `redact`, `cost`, `mcp`. Stubs remaining: `audit`, `drift`. Every command reads a file argument or stdin (`-`), and most take `--json`.

```
tokler count <FILE|-> [--model openai|anthropic|gemini] [--all] [--json]
tokler compare <FILE|->            # token counts across all three
tokler viz <FILE|-> [--model ...] [--max-tokens N] [--json]
tokler redact <FILE|-> [--strict] [--allow KIND]... [--json]
tokler cost <FILE|-> [--model ID] [--output-tokens N] [--calls N]
             [--cache-hit-rate 0..1] [--cache-ttl 5m|1h] [--batch-fraction 0..1] [--json]
tokler mcp <FILE|-> [--provider openai|anthropic|gemini] [--json]
```

### Web (`apps/tokler-web`, Next.js 16.2.6 + React 19, Turbopack)

The home page (`/`) renders, in order: the tokenizer flow (a shared-state textarea + file dropzone, the redaction ledger, the 3-provider count panel, the token-chip visualizer, the cost calculator), then a divider, then the standalone MCP analyzer. Redaction runs first; everything downstream analyzes the redacted text.

### Core (`packages/tokler-core`)

Modules: `pricing`, `cost`, `redact`, `mcp`. 33 unit tests. WASM bindings (all take and return JSON strings): `version()`, `redact(text, options_json)`, `cost(request_json)`, `models()`, `analyze_mcp(config_text, provider)`. WASM artifact is about 937 KB (the regex engine in `redact` dominates).

Pricing model ids: `claude-opus-4.8`, `claude-opus-4.7`, `claude-sonnet-4.6` (Anthropic default), `claude-haiku-4.5`, `gpt-5.5`, `gpt-5.4` (OpenAI default), `gpt-5.4-mini`, `gpt-5.4-nano`, `gemini-2.5-pro` (Gemini default), `gemini-2.5-flash`, `gemini-2.5-flash-lite`.

---

## 6. File map (the important ones)

```
packages/tokler-core/
  Cargo.toml                         workspace (members: crates/core, crates/wasm)
  deny.toml                          license allow-list (MIT/Apache/Unicode-3.0/...)
  package.json                       exposes pkg/ as tokler-core-wasm
  crates/core/src/
    lib.rs                           Provider enum + version() + module decls
    pricing.rs                       ModelPrice table + find/default_for/all
    cost.rs                          CostRequest -> CostBreakdown
    redact.rs                        redact(text, opts) -> RedactionResult
    mcp.rs                           analyze(config, provider) -> McpAnalysis
  crates/wasm/src/lib.rs             wasm-bindgen wrappers (JSON string in/out)
  pkg/                               wasm-pack output (gitignored, rebuilt by turbo)

apps/tokler-cli/
  Cargo.toml                         path-deps tokler-core; tiktoken-rs, tokenizers, etc.
  assets/gemma-tokenizer.json        embedded Gemma vocab (~17.5 MB)
  src/main.rs, cli.rs, input.rs
  src/parse/mod.rs                   file ingestion (PDF/DOCX/XLSX/text)
  src/tokenize/mod.rs                Provider enum, count(), segments()
  src/commands/{count,compare,viz,redact,cost,mcp}.rs   implemented
  src/commands/{audit,drift}.rs                          stubs

apps/tokler-web/
  PLAN.md PROGRESS.md CLAUDE.md AGENTS.md HANDOFF.md     docs
  src/app/
    page.tsx                         server component: <Analyzer/> + <McpAnalyzer/>
    analyzer.tsx                     client wrapper: text state, redaction-runs-first
    tokenizer-panel.tsx              3-provider live counts
    visualizer.tsx                   token chips (OpenAI/Gemini) + Claude band
    cost-panel.tsx                   cost calculator UI
    redaction-ledger.tsx             findings ledger (no secret bytes)
    mcp-analyzer.tsx                 the MCP analyzer panel
    file-drop.tsx                    drag/drop file ingestion
  src/lib/tokenize/{index,openai,anthropic,gemini,types,client,worker}.ts
  src/lib/core/{index,types,client,worker}.ts            WASM client (redact/cost/models/analyzeMcp)
  src/lib/parse/index.ts             web file parsing (pdfjs/mammoth/xlsx)
```

---

## 7. Decisions and gotchas (the non-obvious stuff)

- Claude tokenization is a cl100k_base proxy, always labeled an estimate with a +/-10% band. The "port bpe-lite" idea was dropped as a no-op (bpe-lite is Node-only and uses cl100k_base anyway). Exact counts wait for the Phase 2b hybrid.
- WASM needs two wasm-opt feature flags in `crates/wasm/Cargo.toml`: `--enable-bulk-memory` (modern Rust emits bulk-memory ops) and `--enable-nontrapping-float-to-int` (the cost calculator's float-to-int casts emit `i64.trunc_sat_f64_u`). Without these, the wasm-pack-bundled wasm-opt rejects the binary.
- WASM crosses the JS boundary as JSON strings (no `serde-wasm-bindgen` dependency, keeping the tree shallow). The web client JSON.stringifies inputs and JSON.parses outputs into types that mirror the serde structs one-to-one (snake_case preserved on purpose).
- Redaction `--strict` RAISES the confidence threshold (0.7 vs 0.5), meaning fewer and higher-precision redactions. This matches the PLAN wording; document it wherever it surfaces.
- `example.com` and any `EXAMPLE`-bearing string is suppressed as a placeholder (RFC 2606). Tests must use non-example hosts for db-uri fixtures.
- The MCP catalog is data, not code (refreshable). Cold-cache token costs are representative estimates. Unknown servers are flagged with a prompt to paste their `tools/list`, never guessed.
- `cargo fmt --check` on the core workspace needs `--all` (the workspace `Cargo.toml` is virtual). `--manifest-path <virtual Cargo.toml>` alone fails with "Failed to find targets."
- Browser verification: there is a Claude Preview MCP available. Create `.claude/launch.json` with a `tokler-web` config pointing at `bun run --filter=tokler-web dev` (port 3000), call `preview_start`, then `preview_eval` / `preview_screenshot` / `preview_console_logs`. Remove `.claude/launch.json` afterward (it is a local verification helper, not committed). This is how the WASM-in-worker runtime (redaction, cost, MCP analysis) was proven; the build and typecheck alone do not exercise the post-hydration WASM path.
- Per-phase ritual: implement, run all gates (cargo test/clippy/fmt, bun lint/lint:fast/typecheck/build, dash scan, privacy scan, `bun pm untrusted`), browser-verify when a UI changed, update PROGRESS.md, commit (no AI attribution), push.

---

## 8. How to build and verify

From the repo root:

```bash
# Core (Rust): tests, lints, format, wasm
cargo test  --manifest-path packages/tokler-core/Cargo.toml
cargo clippy --manifest-path packages/tokler-core/Cargo.toml --all-targets -- -D warnings
cargo fmt   --all --manifest-path packages/tokler-core/Cargo.toml --check
(cd packages/tokler-core && wasm-pack build crates/wasm --target web --out-dir ../../pkg --release)

# CLI (Rust)
cargo test  --manifest-path apps/tokler-cli/Cargo.toml
cargo clippy --manifest-path apps/tokler-cli/Cargo.toml --all-targets -- -D warnings
(cd apps/tokler-cli && cargo build --release)   # ./target/release/tokler

# Web
bun install
bun run --filter=tokler-web lint        # Biome
bun run --filter=tokler-web lint:fast   # oxlint
bun run --filter=tokler-web typecheck   # tsgo
bun run --filter=tokler-web build       # Turbopack
bun run --filter=tokler-web dev         # local server on :3000

# Hygiene scans (must be clean)
rg -n "[\x{2014}\x{2013}]" packages/tokler-core/crates apps/tokler-cli/src apps/tokler-web/src   # em-dash (U+2014) / en-dash (U+2013): expect zero hits
grep -rIn "localStorage\|indexedDB\|fetch(" apps/tokler-web/src                                    # no egress
bun pm untrusted                                                                                    # 0
```

Current state: core 33 tests pass, CLI 17 tests pass, web all gates pass, scans clean, 0 untrusted deps.

---

## 9. What is next (roadmap)

### Phase 2b (the rest of "the wedge")
- Audit rules engine: the Lighthouse-style ranked findings over redacted text. Each finding has severity, an input-token savings range, a confidence interval, a "production proven" or "experimental" badge, and a citation. Start with the production-proven detections in PLAN section 8 (near-duplicate paragraphs via MinHash + LSH, over-quoted sources, MCP tool-description bloat, JSON verbosity, stack-trace verbosity, cache-miss risk from volatile prefixes, HTML-to-Markdown, repeated file reads). CLI `tokler audit`.
- Opt-in Anthropic `count_tokens` hybrid: a "Verify with Anthropic" action that calls `/v1/messages/count_tokens` directly from the browser with the `anthropic-dangerous-direct-browser-access: true` header, using the user's own key held in local memory, sending only redacted text, cached by SHA-256(content + model + tools). Shows the local-vs-API delta. CLI equivalent via `reqwest` (rustls).
- Tokenizer-drift comparator: the same input across model versions (Claude 4.5 / 4.6 / 4.7 / 4.8, GPT and Gemini versions). The headline is "this prompt costs N% more on Opus 4.7 than 4.6 because of a tokenizer change." CLI `tokler drift`.

### Phase 3 (frontier)
LLMLingua-2 in-browser compression preview (`@atjsh/llmlingua-2`), Model2Vec static embeddings for semantic dedup, the experimental detections (badged with confidence intervals), and format-efficiency previews (HTML to Markdown, JSON to TOON).

### Phase 4 (distribution and community)
MIT open-source release, a GitHub Action / Bun script that runs `tokler audit` on PR diffs, a community-curated MCP catalog moved to its own repo, a plug-in interface for additional providers, and a hosted demo (still 100% client-side).

---

## 10. Quick orientation for a new agent

1. Read root `/CLAUDE.md`, then `apps/tokler-web/CLAUDE.md`, then `apps/tokler-web/PLAN.md`.
2. Skim `apps/tokler-web/PROGRESS.md` from the bottom up for the latest state.
3. The Rust core is the place to add shared logic. Add a module under `crates/core/src/`, expose it through `crates/wasm/src/lib.rs` (JSON string in/out), wire it into the CLI under `src/commands/`, and consume it on the web through `src/lib/core/`.
4. Respect the rules: bun only, no em-dashes, no AI attribution in commits, no data egress, input-bounded claims, no fabricated Claude token boundaries.
5. Verify with the gates in section 8, browser-verify any UI change, update PROGRESS.md, then commit and push.
