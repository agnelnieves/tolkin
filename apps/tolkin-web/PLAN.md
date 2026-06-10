# Tolkin Master Plan

A privacy-first AI token analysis and optimization advisor. Runs entirely in the browser. Also ships as a CLI for SDLC pipelines and one-shot `bunx` / `npx` workflows. Analyzes prompts, configs, MCP setups, and docs across the three frontier providers (OpenAI, Anthropic, Google) and recommends savings with quantified $ impact, confidence intervals, and citations.

This plan synthesizes two parallel research passes (an 8-agent deep search + a comprehensive technical research document) into one decisive action path. Where the two diverge, the resolution is captured here with a short reason.

## 0. Executive summary

1. **Three deliverables, one Rust core.** A shared Rust crate (`packages/tolkin-core`) holds the rules engine, MCP analyzer, cost calculator, secret redactor, and near-duplicate algorithms. It compiles to WASM for the browser and is linked natively into the CLI binary. Tokenization stays platform-native (JS libs in the browser, Rust crates in the CLI) because reinventing BPE/SentencePiece for cross-platform parity is poor ROI.
2. **Privacy-first with one opt-in hybrid lever.** OpenAI and Gemini tokenization are 100% offline (both are exact in the browser). Claude defaults to a labeled offline approximation (~9% mean error per `bpe-lite`); a per-session opt-in "Verify with Anthropic" button calls `/v1/messages/count_tokens` directly from the browser using the documented `anthropic-dangerous-direct-browser-access: true` header. The endpoint is free, has CORS, and is rate-limited separately from message creation.
3. **Headline wedge: the MCP analyzer.** Paste a Claude Desktop, Cursor, Claude Code, or VS Code Copilot MCP config and get cold-cache, warm-cache, and Tool-Search-enabled token cost across providers, plus CLI swap recommendations with documented savings. This sits in genuine whitespace; no marketplace or inspector currently quantifies it.
4. **Recommendations ship as a Lighthouse-style audit.** Each finding has severity, estimated input-token savings range, confidence interval, a "production proven" or "experimental" badge, and a citation link. Honesty is the trust moat.
5. **CLI parity from day one.** `tolkin count`, `tolkin audit`, `tolkin mcp`, `tolkin cost`, `tolkin viz`, `tolkin redact`. Distributed as a Rust binary via Cargo and as `@tolkin/cli` npm wrapper (downloads precompiled binary on first run, like esbuild). `bunx @tolkin/cli` and `npx @tolkin/cli` both work.
6. **Open source from the start.** MIT license, public-friendly naming, no links to the private personal-site monorepo from any user-facing surface. Buildable from a clean checkout with `cargo build` + `bun install` + `bun run build`.

## 1. Identity and scope

**Name:** Tolkin (renamed 2026-06-10 from Tokler, the owner's branding call made before any public surface shipped; the lineage runs Tokenist, dropped for an SEO collision with the established tokenist.com, then Tokler, whose bare npm name was rejected for similarity to howler, then Tolkin. The Tolkien-trademark adjacency was flagged and consciously accepted. Old tokler-* npm packages are deprecated pointers, never reused).

**Tagline candidates:** "Audit your prompts. Nothing leaves your browser." or "See every token before you spend a dollar."

**Primary audience (now):** SDLC pipeline owners optimizing AI agent token spend. At ~15 PRs / engineer / week, agent setups have been measured at ~400K input tokens per PR with re-sent context accounting for 62% of the bill (TrueFoundry, GitHub Engineering). This is where the biggest wins live.

**Secondary audience:** Anyone iterating on prompts who wants real cost numbers, not a tokenizer toy that says "this is 1,247 tokens, hope that helps."

**In scope (v1):**
- Frontier model tokenization (OpenAI, Anthropic Claude, Google Gemini)
- File ingestion: PDF, DOCX, XLSX, Markdown, YAML, TOML, JSON / JSONC, code files, `.env`
- MCP config analysis (Claude Desktop, Claude Code, Cursor, VS Code Copilot, Continue.dev, Zed)
- Secret detection and redaction (gitleaks + secrets-patterns-DB ported regexes + entropy)
- Cost calculator with cache / batch awareness
- Audit (the rules engine with production-proven detections)
- Tokenization visualizer (no fabricated visualization for Claude)
- Tokenizer-drift comparator (the Opus 4.7 1.0x-1.35x story is a real feature)
- CLI parity for every web surface

**Out of scope (v1, deferred to later phases):**
- **Transformation.** Tolkin analyzes and recommends. It does not rewrite the user's prompts in v1. Phase 3 brings actual compression preview via LLMLingua-2 in-browser.
- **Open-weight tokenizers** (Llama, Mistral, DeepSeek, Qwen, gpt-oss). The architecture is extension-friendly because `@huggingface/transformers` plugs in cleanly, but v1 ships only the three frontier providers per the original product spec.
- **ML-based dedup.** Heuristics (MinHash + LSH + TF-IDF) cover the dominant value cases. Model2Vec static embeddings are a Phase 3 opt-in upgrade.
- **Server-side anything.** No backend, no database, no telemetry. Saved scenarios use opt-in IndexedDB with an explicit consent screen.
- **WebLLM-style local inference.** Right tool for "rewrite my prompt"; wrong tool for "audit it."

## 2. Architecture

```
                                    +---------------------------+
                                    |   packages/tolkin-core   |
                                    |   (Rust crate)            |
                                    |                           |
                                    |  - rules engine           |
                                    |  - MCP analyzer           |
                                    |  - cost calculator        |
                                    |  - secret redactor        |
                                    |  - MinHash / SimHash      |
                                    |  - pricing tables         |
                                    +-------+-----------+-------+
                                            |           |
                              wasm-bindgen  |           |  rlib (native)
                                            |           |
                          +-----------------v-+       +-v-----------------+
                          | tolkin-core-wasm |       |  apps/tolkin-cli |
                          | (npm package)     |       |  (Rust binary)    |
                          +---------+---------+       +---------+---------+
                                    |                           |
                                    | TS import                 | clap CLI
                                    |                           |
                          +---------v---------+                 |
                          | apps/tolkin-web  |                 |
                          | (Next.js 15)      |                 |
                          | + Web Workers     |                 |
                          | + pdf.js / mammoth|                 |
                          +-------------------+                 |
                                                                |
                                                                v
                                                    +-----------+-----------+
                                                    | @tolkin/cli (npm)    |
                                                    | platform-detect       |
                                                    | + binary downloader   |
                                                    +-----------------------+
                                                    bunx / npx @tolkin/cli
```

**Why Rust core + WASM + native, instead of pure TypeScript or pure Rust:**
- The CLAUDE.md Rust-first principle. Hot paths (regex over megabytes of paste, MinHash over thousands of paragraphs, MCP tool inventory tokenization) belong in Rust.
- Single source of truth for the high-value logic (rules, MCP catalog, pricing tables). One regression test suite, two surfaces.
- Tokenization stays per-platform because BPE/SentencePiece libs are mature, fast, and large; reinventing them in Rust for WASM costs more than it saves.

**Why tokenization is platform-native:**
- Browser: `gpt-tokenizer` (sync, ~50 KB min+gz per encoding) for OpenAI; HF Gemma tokenizer via `@huggingface/transformers` for Gemini; `bpe-lite` for Claude approximation. All three are battle-tested, sub-millisecond on typical inputs.
- CLI: `tiktoken-rs` for OpenAI; `sentencepiece` Rust crate for Gemini (load Gemma SPM model file); same `bpe-lite` logic ported once to Rust (~300 lines).
- The shared core treats token counts as a primitive input. It never tokenizes; surfaces do.

## 3. Monorepo layout

```
.
├── apps/
│   ├── cli/                   # existing: portfolio CLI (untouched)
│   ├── web/                   # existing: agnelnieves.com (untouched)
│   ├── tolkin-web/           # NEW: Next.js 15 web UI for Tolkin
│   │   ├── PLAN.md            # this file
│   │   ├── README.md
│   │   ├── package.json
│   │   ├── next.config.ts
│   │   ├── biome.json
│   │   └── src/
│   │       ├── app/
│   │       ├── components/
│   │       ├── lib/
│   │       │   ├── tokenizers/
│   │       │   ├── parsers/
│   │       │   └── workers/
│   │       └── types/
│   └── tolkin-cli/           # NEW: Rust binary
│       ├── Cargo.toml
│       ├── README.md
│       ├── src/
│       │   ├── main.rs
│       │   ├── cli.rs
│       │   ├── commands/
│       │   └── format.rs
│       └── npm/               # npm wrapper for bunx / npx
│           ├── package.json
│           └── postinstall.js
└── packages/
    ├── tsconfig/              # existing
    └── tolkin-core/          # NEW: Rust crate + WASM artifact
        ├── Cargo.toml
        ├── README.md
        ├── crates/
        │   ├── core/          # rlib for CLI
        │   └── wasm/          # cdylib via wasm-bindgen
        ├── src/
        │   ├── lib.rs
        │   ├── rules/
        │   ├── mcp/
        │   ├── cost/
        │   ├── redact/
        │   ├── minhash/
        │   └── pricing.rs
        └── pkg/               # wasm-pack output (consumed by tolkin-web)
```

**Workspace registration.** Add `apps/tolkin-web` and `apps/tolkin-cli` to the root `package.json` `workspaces` array (already covered by `apps/*` glob). Add a top-level `Cargo.toml` workspace member entry for `apps/tolkin-cli` and `packages/tolkin-core/crates/core` and `packages/tolkin-core/crates/wasm`.

**Turbo pipeline.** Each new workspace ships its own `turbo.json` extending `//`. The Rust core builds via `cargo build --release` and `wasm-pack build`; the web app's `dev` script depends on the WASM pkg existing in `packages/tolkin-core/pkg`. CI runs `cargo clippy --all-targets -- -D warnings` and `cargo deny check`.

## 4. Tech stack (per workspace)

### `packages/tolkin-core`

- **Rust edition 2024.** `[profile.release] lto = true, codegen-units = 1, strip = true, panic = "abort", opt-level = "z"` to match the portfolio CLI's discipline.
- `regex = "1"` for redaction patterns.
- `serde` / `serde_json` for JSON / JSONC config parsing.
- `serde_yaml_ng` (maintained fork; not the unmaintained `serde_yaml`).
- `toml = "0.8"` for TOML configs.
- `aho-corasick` for fast multi-pattern keyword prefiltering before entropy.
- `simhash` or hand-rolled MinHash + LSH (5-character shingles).
- `wasm-bindgen` + `js-sys` + `web-sys` for WASM bindings.
- `cargo deny` allow-list extended to cover new crate licenses.

### `apps/tolkin-cli`

- Rust binary, `clap = "4"` with `derive` for ergonomic subcommands.
- `tiktoken-rs` for OpenAI tokenization.
- `sentencepiece` (or `tokenizers` from Hugging Face) for Gemini's Gemma SPM. Bundled Gemma `tokenizer.model` (~4 MB) as a release artifact, fetched on first run and cached.
- `reqwest` (rustls, no openssl) for the optional Anthropic `count_tokens` hybrid call.
- `pdf-extract` or `mupdf` Rust binding for PDF text extraction.
- `dunce` + `directories` for OS-correct config paths (`~/.config/tolkin/config.toml` on Linux, etc.).
- Output: human-readable in TTY, JSON with `--json` flag.

### `apps/tolkin-web`

- Next.js 15 (App Router), Turbopack (no `--webpack` regression).
- Tailwind v4 with Lightning CSS.
- Bun 1.3.x (matches root); `bun add` only, no `npm install`. No `trustedDependencies` additions unless required.
- Biome 2 + oxlint (no Prettier, no ESLint).
- `tsgo` (TS 7 Go-based preview) for type checking.
- `gpt-tokenizer@^3.4` for OpenAI (per-encoding code splitting, lazy import).
- `@huggingface/transformers@^4` for Gemini Gemma tokenizer; loaded in a Web Worker on first use.
- `bpe-lite` for Claude offline approximation.
- `pdfjs-dist@^6` (worker on a separate URL), `mammoth@^1.12`, `xlsx` (SheetJS CE), `jszip` + DIY XML for PPTX, `unified` + `remark-parse` + `remark-gfm` + `remark-frontmatter` + `strip-markdown`, `yaml@^2` (eemeli), `smol-toml`, `jsonc-parser@^3`, `web-tree-sitter@^0.25` (lazy per-grammar), `file-type@^22`, `tesseract.js@^6` (opt-in OCR).
- `comlink` for Web Worker ergonomics.
- React Server Components only where they pay off; the analyzer is fully client-side.
- No analytics in v1. No localStorage / IndexedDB unless the user explicitly enables saved scenarios.

### `apps/tolkin-cli/npm`

- Tiny Node-compatible JS wrapper (~50 LOC).
- On `postinstall`: detect platform (darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64), download the matching precompiled binary from a GitHub release, verify checksum.
- The wrapper's bin script `exec`s the downloaded binary with the user's args.
- Pattern proven by esbuild, swc, biome.
- Optional: also publish per-platform npm packages (`@tolkin/cli-darwin-arm64` etc.) as optional dependencies so Bun's optional-dep resolution skips downloads.

## 5. Tokenization strategy (per provider)

| Provider | Web | CLI | Accuracy | Notes |
|---|---|---|---|---|
| OpenAI | `gpt-tokenizer@^3.4` (sync, in-worker) | `tiktoken-rs` | Exact | Supports `o200k_base` and `o200k_harmony` (GPT-5 chat format with roles/channels). Show "+ reasoning" multiplier toggle for o-series. |
| Gemini | HF Gemma tokenizer via `@huggingface/transformers` | `sentencepiece` Rust crate | Exact | All current Gemini models share the Gemma SPM vocab (262,144 tokens). Lazy-load ~4MB model in Web Worker. Document the June 19, 2026 API-key restriction in BYOK setup. |
| Claude (offline) | `bpe-lite` | Port of bpe-lite logic to Rust (~300 LOC) | Mean ~9%, median ~6%, worst case ~25% (Arabic / emoji) | Label clearly as "~ estimate". Show confidence band in UI. |
| Claude (hybrid, opt-in) | Direct browser call to `/v1/messages/count_tokens` with `anthropic-dangerous-direct-browser-access: true` header | Same endpoint via `reqwest` | Exact | Endpoint is free. Cache by `SHA-256(content + model + tools)` so iterative tuning pays once per unique input. User pastes their own key into local memory; never persisted. |

**Visualization rule.** The token-chip view (Tiktokenizer-style colored boundaries with IDs) renders only when the BPE/SPM vocabulary is real (OpenAI, Gemini). For Claude the UI shows the count, the confidence band, and a "this is an estimate; verify for $0" CTA, but never fabricates per-token boundaries. Fabricating BPE splits would mislead users; reputable tools (e.g., webtoolkit.tech) already skip it.

**Tokenizer-drift comparator.** First-class feature: same input tokenized across Claude 4.5 / 4.6 / 4.7 / 4.8 (and across GPT and Gemini versions). The headline output looks like "your prompt costs 18% more on Opus 4.7 than on 4.6 because of the tokenizer change" with a link to Anthropic's announcement. This is one of Tolkin's most defensible features; the story is real, quantified (1.0x to 1.35x multiplier, up to 35% inflation on code and non-English text), and silently expensive without a tool that surfaces it.

## 6. File ingestion (split by surface)

| File type | Web | CLI |
|---|---|---|
| PDF | `pdfjs-dist@^6` in Web Worker; `tesseract.js@^6` opt-in OCR for scanned pages | `pdf-extract` (or `mupdf` Rust binding) |
| DOCX | `mammoth@^1.12` (`extractRawText`) | `docx-rs` or `mammoth-rs` (or shell out to pandoc if installed) |
| XLSX | `xlsx` (SheetJS CE) | `calamine` crate |
| PPTX | `jszip` + XML walker pulling `<a:t>` | `zip` + `quick-xml` |
| Markdown | `unified` + `remark` + `strip-markdown` (keeps AST and plain text) | `pulldown-cmark` |
| YAML | `yaml@^2` (eemeli) | `serde_yaml_ng` |
| TOML | `smol-toml` | `toml` crate |
| JSON / JSONC | `jsonc-parser@^3` | `serde_json` + `jsonc-parser` Rust port |
| Code | `web-tree-sitter@^0.25` + lazy grammar WASM | `tree-sitter` + per-language crates |
| File sniff | `file-type@^22` | `infer` crate |
| Encoding | `chardet` + `TextDecoderStream` | `encoding_rs` |

**Large file handling (web).** Streams API pipeline: `file.stream().pipeThrough(new DecompressionStream('gzip')).pipeThrough(new TextDecoderStream('utf-8'))`. One persistent Web Worker per parser family. Transfer `ArrayBuffer`s in (zero-copy), strings out. OPFS only for opt-in saved scenarios.

**Bundle discipline (web).** First paint ships only the UI shell, file-type sniff, and base tokenizer (~150 KB JS gz). Every parser, OCR core, and grammar is `await import()` on first use. Per-provider tokenizer chunks lazy-load when their tab activates.

## 7. Secret redaction (always-on, runs first)

The redactor is in `packages/tolkin-core` so the same engine ships in the browser (via WASM) and the CLI. Runs **before** anything else (before display, before analysis, before any opt-in hybrid API call).

**Pattern catalog sources (ported to Rust regex):**
- `gitleaks/gitleaks` `config/gitleaks.toml` (~150 rules, easiest to bundle).
- `trufflesecurity/trufflehog` `pkg/detectors/` (~800 detectors, MIT, deeper coverage).
- GitHub's "Supported secret scanning patterns" page (partner-confirmed regexes for vendors that publish them).
- Yelp `detect-secrets` entropy thresholds (Base64HighEntropyString 4.5 bits/char, HexHighEntropyString 2.7-3.0).

**v1 vendor coverage:** OpenAI (`sk-`, `sk-proj-`, `sk-svcacct-`, `sk-admin-`), Anthropic (`sk-ant-api03-`, `sk-ant-admin01-`, `sk-ant-oat01-`), Google AI (`AIza...`), AWS (AKIA/ASIA/AROA/AIDA/ANPA/ANVA/AIPA), GitHub (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`), Slack (`xox[baprsoe]-`), Stripe (`sk_live_`, `pk_live_`, `rk_live_`), JWT (`eyJ...`), Bearer / Authorization headers, private keys (`BEGIN .* PRIVATE KEY`), DB connection strings (`postgres://`, `mysql://`, `mongodb+srv://` with embedded passwords), Cloudflare (scannable prefix + checksum), Vercel, Supabase, Notion, Linear, Figma, Neon.

**Confidence score:** `0.4 (base) + 0.3 (regex match) + 0.2 (keyword context within 32 chars: "key" / "token" / "secret" / "auth" / "password" / "api_key") + 0.1 (entropy gate passed)`. Anything `>= 0.5` is auto-redacted. Below threshold goes into the ledger as "possible secret, click to review."

**Placeholder format (token-count parity):** single deterministic label per type (`<REDACTED:openai-key>`, `<REDACTED:jwt>`, etc.). Length-padding leaks original secret length; avoid it. Tokenizers count these deterministically, so analysis stays stable.

**False-positive suppression:** `YOUR_API_KEY`, `XXXX`, `REPLACE_ME`, `EXAMPLE`, repeated-char patterns, lorem ipsum, image data URIs (`data:image/`), well-known git SHAs without context, UUIDs, build hashes.

**MCP-config-specific rule:** every value of an `env` or `headers` object is suspect by structural position. Redact even without a regex match if the key name contains "key", "token", "secret", "auth", "password". Same for command-arg patterns like `--api-key <value>` and `--header "Authorization: ..."`.

**Threat model brief:**
- No localStorage, no IndexedDB, no Service Worker cache by default. Original text lives in a React state ref overwritten on next paste.
- Redactor runs in a dedicated Web Worker. Main thread posts raw text in, gets redacted text + ledger out. Worker `terminate()` after analysis is the only reliable way to drop the worker heap.
- Hybrid API mode sends only the redacted text. UI shows the egress payload diff before send with explicit confirm.
- No telemetry. No remote logging. Document this in the hero copy ("Nothing leaves your browser.").

**UI:** `RedactionLedger` sidebar lists each finding with `{type, confidence, start, length, label}`. Click-and-hold to inspect original (no clipboard, no DOM-readable copy). "Restore this match" affordance for the inevitable false positives.

## 8. Analysis engine: detection catalog

The audit runs over redacted text and produces a Lighthouse-style ranked issue list. Each issue carries:

- **Severity** (critical / high / medium / low) based on potential savings size.
- **Estimated input-token savings range** (e.g., "saves 1,200-3,800 tokens").
- **Estimated $ savings** at the user's configured monthly volume.
- **Confidence interval** (e.g., "high confidence, 95% of detections are real").
- **Badge:** "Production proven" or "Experimental."
- **Citation link** to the paper, repo, or blog post backing the detection.

### Production-proven (ship in Phase 2, low false-positive risk)

| Detection | Method | Expected savings | Backing |
|---|---|---|---|
| Near-duplicate paragraphs | MinHash + LSH on 5-char shingles, Jaccard >= 0.7 | 10-40% on user-pasted blobs | Standard dedup literature |
| Over-quoted source documents | Verbatim quotes >200 tokens of attached URLs / files | 50-90% per finding | RECOMP (arxiv:2310.04408), LongLLMLingua (arxiv:2310.06839) |
| MCP tool-description bloat | Count tools, schema bytes, tools-mentioned vs tools-listed | 30-60% per request | GitHub Agentic Workflows; Anthropic context-engineering |
| JSON / structured-data verbosity | Pretty-vs-minified delta, long recurring keys | 60-90% on pretty JSON; 30-60% via TOON on uniform arrays | curiouslychase GPT-4 benchmark; Schmidt et al. (W&M) |
| Stack-trace verbosity in agent context | Regex stack patterns, dedup across turns | 50-80% per stack | Vaughan ACC; claude-code GitHub #42647 |
| Code-formatting whitespace cost | Strip non-semantic whitespace estimator (NOT for Python / YAML) | ~24.5% input tokens | arxiv:2508.13666 |
| Prompt-cache miss risk (volatile prefix) | Detect timestamps / user IDs / session data early in prompt | Up to 90% via cache engagement | Anthropic prompt-caching docs |
| Sub-cache-threshold prompts | Total cacheable prefix < 1024 tokens | 50-90% on cacheable portion if padded | OpenAI "Prompt Caching 201" |
| HTML to Markdown | If input is HTML, estimate Markdown equivalent | 20-94% (boilerplate-dependent) | SearchCans, Web2MD, Sanity case study |
| Repeated file reads (multi-turn) | Same content appearing in >1 message in supplied conversation history | Up to 54% via cache hits | Vaughan ACC |
| Tokenizer-version drift | Compare same input across model versions | Up to 35% silent inflation on Opus 4.7 | Anthropic Opus 4.7 release notes |

### Experimental (Phase 3, surface confidence interval)

| Detection | Method | Expected savings | Backing |
|---|---|---|---|
| Semantic-cluster duplication | Model2Vec / MiniLM embeddings + cosine >= 0.85 | ~30% on RAG contexts; FP ~15-20% | LlamaIndex SemanticSplitter; ChunkRAG |
| Low-self-information spans | Stopword density × syntactic-shape ratio, or small-LM perplexity | 30-50% per Selective Context | Selective Context (Li 2023) |
| "Caveman-eligible" passages | (articles + auxiliaries + fillers) / content words > 35% | 50-65% per flagged passage | wilpel/caveman-compression |
| Excessive few-shot examples | Count repeated `Input:/Output:` or `<example>` patterns; flag >5 | 30-50% prompt reduction at often-negligible quality drop | Tetrate plateau studies |
| Markdown vs plain-text tradeoff | Estimate both; flag if delta >15% AND no nested structure | 20-30% (plain text cheaper) | web2md, MDSpin benchmarks |
| Repeated instructions in system prompt | Sentence-level near-dup | 5-20% on typical system prompts | Hu et al. prompt-defect taxonomy (arxiv:2509.14404) |
| Filler-phrase / hedging | Dictionary of fillers ("please note", "it is important to", "very", "really", "just") | 5-15% | Token-optimization blog literature |
| Middle-of-context low-attention zones | Flag middle 30-70% if context >50K tokens | Indirect; reorder for 20-30% accuracy gain | "Lost in the middle" literature |
| Verbose role / persona descriptions | "You are a..." count, flag >3 consecutive sentences | 5-15% on system prompts | Anthropic Claude 4 best practices |
| Compaction amplification risk | Same observation appearing in multiple messages (multi-turn input) | Up to 54% via ACC | Vaughan ACC deep-dive |

### Honest limits (be explicit in UI)

The pre-registered randomized trial (arxiv:2603.23525, March 2026) on 358 Claude Sonnet 4.5 runs found that **moderate compression (r=0.5) cut total cost 27.9%, but aggressive compression (r=0.2) INCREASED total cost by 1.8%** because output tokens grew. **All Tolkin savings claims are input-token bounded, not total-cost.** The UI says "input savings, output may vary" everywhere a number appears.

## 9. MCP analyzer (the headline wedge)

Five-step algorithm, implemented in `packages/tolkin-core::mcp`:

1. **Parse config.** Normalize the four common shapes:
   - `mcpServers: { name: { command, args, env, url? } }` (Claude Desktop, Cursor, Continue.dev, Claude Code)
   - `servers:` root key (VS Code Copilot)
   - `context_servers:` (Zed)
   - Claude Code project file `.mcp.json`

2. **Resolve each server to a tool inventory.** Three paths, accuracy-ordered:
   - **(a) Live probe (CLI only, opt-in).** Spawn the stdio server (or hit SSE/HTTP URL), call `initialize` then `tools/list`, capture JSON.
   - **(b) Curated static catalog.** Bundled lookup table covering the top ~50 servers (GitHub, Filesystem, Postgres/Neon, Supabase, Notion, Linear, Slack, Jira, AWS, Kubernetes, XcodeBuildMCP, Figma, Google Drive/Gmail/Calendar, Brave Search, Sentry, etc.). Catalog format is per-server fixture JSON, refreshable.
   - **(c) User-supplied paste.** "We don't have this server's tool list cached. Paste its `tools/list` output here." Tokenize directly.

3. **Tokenize the serialized array.** Each tool as `{name, description, input_schema}` JSON, no whitespace beyond the client's default serialization. Important: count tools once, not per-tool with duplicated boilerplate (the bug Anthropic's `/context` had until January 2026 that inflated XcodeBuildMCP from 12.6K to 45K).

4. **Produce three scenarios per server:**

All cache multipliers below derive from the `tolkin-core::pricing` table at runtime (cold = `cache_write_5m / input`, warm = `cache_read / input`, both fall back to `1.0` when the provider publishes no separate rate). The table is the single source of truth; updating a pricing row updates the analyzer automatically.

| Scenario | Anthropic math | OpenAI math | Gemini math |
|---|---|---|---|
| Cold cache (first turn) | `tokens × 1.25` (write surcharge, derived) | `tokens × 1.0` (no write surcharge published) | `tokens × 1.0` (no write surcharge published) |
| Warm cache (subsequent turn) | `tokens × 0.10` (90% off, derived) | `tokens × 0.10` (cached input is 10% of base across GPT-5) | `tokens × 0.10` (Gemini 2.5 family cached at 10% of base, verified 2026-06) |
| With Tool Search defer-loading | `~500` (search stub) `+ min(tools, 5) × 600` loaded on demand | Same | Same |
| Capacity cost (always) | `tokens` (attention slots) | `tokens` (attention slots) | `tokens` (attention slots) |

5. **Sum and report.** Per-server breakdown plus totals across cold session, warm session, Tool-Search session, and percent of 200K window consumed. Convert to $ at the user's monthly volume.

**Pre-populated catalog (cold-cache estimates, refreshable):**

| MCP server | Tools | Cold tokens | CLI alternative | Recommendation |
|---|---|---|---|---|
| GitHub (official) | 90-162 | 26-55K | `gh` CLI | Replace by default; agent already knows `gh` |
| Filesystem | 11 | 1-3K | Bash + `find`/`cat`/`rg` | Replace |
| Postgres / Neon | 20 | 4-8K | `psql` / `neonctl` | Replace for ad hoc; keep for OAuth flows |
| Supabase | 20+ | ~4-6K | `supabase` CLI | Replace |
| Notion (official) | 21 | ~26K | none mature | Keep MCP, or use `notion-slim` (52% reduction) |
| Linear | 42 | ~13K | `linear` CLI | Replace (65x cheaper per task in Scalekit benchmark) |
| Slack | 30+ | ~21K | `slack-cli` | Replace unless realtime / Socket Mode |
| Jira / Atlassian | varies | ~17K | `jira-cli` | Replace |
| AWS | varies | 5-30K | `aws` CLI | Always CLI |
| Kubernetes | varies | 5-15K | `kubectl` | Always CLI |
| XcodeBuildMCP | ~60 | ~12.6K | `xcodebuild` / `xcrun simctl` | Replace for builds; keep for log streaming |
| Figma (read) | ~15 | 4-8K | none | Keep MCP |

**When to keep MCP (do not blanket-recommend CLI):** OAuth-gated APIs without a mature CLI (Notion, Figma), realtime / Socket Mode flows (Slack RTM), enterprise auth governance via MCP gateway, proprietary internal APIs the LLM was not trained on.

**Tool Search awareness.** Anthropic shipped Tool Search Tool (defer-loading) in January 2026. With `defer_loading: true`, MCP cost drops to a ~500-token search stub plus 3-5 on-demand tools per task. Tolkin asks "Is your client on a Tool-Search-compatible version?" and shows a third scenario alongside cold/warm, computed as `500 + min(tools, 5) × 600` so the analyzer never disagrees with itself between a per-server note and the column it ships. For the GitHub MCP that scenario lands around 3.5K (90 tools, 5 loaded on demand); external Scalekit benchmarks have reported figures closer to ~8.7K under different prompt shapes, which is why per-server notes attribute any external number explicitly rather than mixing it with the column.

**Why this is the wedge.** No marketplace (Smithery, Glama) and no inspector (`@modelcontextprotocol/inspector`) currently quantifies token cost from a config file. Anthropic's own engineering blog reports tool definitions can consume 72% of a 200K window across just three servers, with $1,370 / dev / year overhead. Scalekit measured GitHub MCP at 1,365 vs 44,026 tokens (32x) versus `gh` for the same task with worse reliability (72% vs 100%). Tolkin is the first tool that walks a user from "paste your config" to "you can save $X / year by swapping these three servers for CLIs."

## 10. Cost calculator

**Pricing tables** (mid-2026, baked into the Rust core, refreshed quarterly via a vendored JSON file):

- **Anthropic:** Opus 4.7 / 4.8 ($5 / $25 input/output, $0.50 cache read, $6.25 5m cache write, $10 1h cache write); Sonnet 4.6 ($3 / $15, $0.30 cache read); Haiku 4.5 ($1 / $5, $0.10 cache read).
- **OpenAI:** GPT-5.5 ($5 / $30); GPT-5.4 ($2.50 / $15); GPT-5.4-mini ($0.75 / $4.50); GPT-5.4-nano ($0.20 / $1.25); cached input ~10% of base across the family.
- **Gemini:** 2.5 Pro ($1.25 / $10 below 200K, $2.50 / $15 above); 2.5 Flash ($0.30 / $2.50); 2.5 Flash-Lite ($0.10 / $0.40).

**Toggles surfaced in the UI:**
- Cache hit rate (or "unknown" with prefix-stability heuristic)
- 5m vs 1h Anthropic TTL
- Batch API (50% off, 24h SLA) eligible portion
- Reasoning effort (low / medium / high) for o-series and GPT-5 thinking
- Anthropic extended thinking on/off (billed at output rate)
- Image count and resolution per call
- Long-context exposure (% of calls >128K, >200K)
- Inference geo (`us` adds 1.1x on Anthropic)

**Worked example surfaced in the UI** (Sonnet 4.6, 20K-token system prompt, 100 calls/hour):
- No cache: $6.00/hr
- 5m caching: $1.43/hr (76% savings)
- 1h caching: $0.71/hr (88% savings)

**Hidden costs the calculator must surface:**
- Reasoning tokens billed at output rate, invisible in response body, discarded between turns
- Anthropic extended thinking billed at output rate (`display: "omitted"` reduces latency, not cost)
- Gemini long-context cliff: hard 2x jump at 200K on both input and output
- Audio (GPT-realtime-2 input audio) at 6.4x text input on the same model
- Opus 4.7 tokenizer drift: stealth ~35% per-character price increase vs 4.6

**Output:input PRICE ratios** (the biggest savings lever, because output is 5-8x more expensive than input per token):
- Anthropic: 5x
- OpenAI: 6x
- Gemini Pro: 8x

These figures describe price, not volume: a Sonnet output token costs 5x what a Sonnet input token costs. They are NOT an estimate of how many output tokens a typical response will contain. The calculator's default per-call total is therefore input-side only (output tokens 0, output cost 0). Passing an output token count or opting in to `estimate_output` (UI checkbox; CLI `--estimate-output`) reproduces the legacy rough-volume assumption (input times the price ratio) and labels every figure that depends on it as such. This keeps the headline number aligned with the product's input-token-bounded identity.

## 11. CLI surface and distribution

**Subcommands:**

```
tolkin count <FILE_OR_STDIN> [--model <model>] [--all] [--json]
    Print token counts. - reads stdin.
    --all compares across OpenAI / Anthropic / Gemini in one table.

tolkin viz <FILE_OR_STDIN> [--model <model>] [--max-tokens <n>] [--json]
    Tokenized view with color-coded boundaries in TTY, JSON with --json.
    For Claude, prints count + estimate band but no fabricated boundaries.

tolkin audit <FILE_OR_STDIN> [--severity <level>] [--rule <id>] [--json]
    Run the rules engine. Print findings with savings estimates.
    Filter by severity (critical/high/medium/low) or specific rule.

tolkin mcp <CONFIG_FILE> [--live-probe] [--client <claude-desktop|cursor|...>] [--json]
    Analyze MCP config. Show cold / warm / Tool-Search-enabled scenarios.
    --live-probe spawns each server and calls tools/list for exact counts.

tolkin cost <FILE_OR_STDIN> --volume <usage.json> [--model <model>] [--cache-hit-rate <0-1>]
    Cost calculator with monthly volume input.

tolkin redact <FILE_OR_STDIN> [--strict] [--allow <type>] [--json]
    Strip secrets. Print redacted text. --strict raises confidence threshold.

tolkin drift <FILE_OR_STDIN> [--models <list>]
    Compare token counts across model versions of the same family.
    Default: claude-3-7 / 4.5 / 4.6 / 4.7 / 4.8.

tolkin compare <FILE_OR_STDIN>
    Side-by-side token counts and estimated $ cost across all three providers.
```

**Configuration:** `~/.config/tolkin/config.toml` (Linux/macOS), `%APPDATA%\tolkin\config.toml` (Windows). Stores API keys for hybrid mode (Anthropic / Gemini), default model preference, custom pricing overrides. Never logged.

**Distribution:**
- **Cargo:** `cargo install tolkin` (when the crate is public).
- **Homebrew:** `brew install tolkin` via a tap once the project hits 1.0.
- **npm wrapper:** `bunx @tolkin/cli` or `npx @tolkin/cli`. The npm package is a ~50 LOC Node-compatible JS shim that detects the platform on `postinstall`, downloads the matching precompiled binary from a GitHub release, verifies a checksum, and proxies all args. Optional per-platform optional dependencies (`@tolkin/cli-darwin-arm64`, etc.) so Bun's optional-dep resolution can skip downloads when the right binary is already present.
- **GitHub releases:** prebuilt binaries via `cargo-dist` for darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64.

**SDLC integration (Phase 4):** `tolkin audit` runs in CI; if any critical / high findings, fail the build or post a PR comment via `gh pr comment`. Same engine as the web; the CI surface is one Bun script call away.

## 12. Open-source readiness

- **License:** MIT (consistent with the surrounding ecosystem; permissive enough for vendor adoption).
- **No links to the private monorepo.** Tolkin's READMEs and documentation reference only `github.com/<future-public-org>/tolkin` when the project is extracted. Until then, public-facing copy describes the codebase generically.
- **Buildable from a clean checkout** with `cargo build` + `bun install` + `bun run build`. No hardcoded paths to the parent monorepo; the Rust crate is self-contained, the web app declares its own dependencies, and the only shared package is `@repo/tsconfig` (which can be inlined when extracted).
- **Public-friendly naming.** "Tolkin" is a clean choice; available domains and npm scope should be verified before any public announcement.
- **Plug-in points:** the tokenizer interface accepts arbitrary providers (open-weight models slot in via `@huggingface/transformers` later); the MCP catalog is data, not code (community-curatable); the rules engine takes rule packs as input.
- **Engraph context.** The shared Rust core is registered in `.engraph/context/` with module-level conventions so future agents working in the codebase have grounded context. Run `/context-add` for any non-obvious decisions captured during implementation.

## 13. Phased roadmap

### Phase 0: Foundation (1 short cycle)

- Scaffold `apps/tolkin-web`, `apps/tolkin-cli`, `packages/tolkin-core` with Cargo workspaces, Turbo tasks, Biome / oxlint / tsgo wired up.
- Empty WASM bindings (`tolkin_core::version()` returns a string) consumed by the web app to prove the integration works.
- CI: `cargo clippy --all-targets -- -D warnings`, `cargo deny check`, `bun run lint`, `bun run typecheck`, `bun run build`.
- README skeletons for each workspace.
- License files.

### Phase 1: Deterministic core (the MVP)

- **Tokenization** for OpenAI (exact), Gemini (exact), Claude (offline approximation with confidence band) on both web and CLI.
- **File parsing** for PDF, DOCX, XLSX, MD, YAML, TOML, JSON / JSONC on both surfaces.
- **Secret redaction** (Rust core, runs first, ledger UI on web).
- **Tokenization visualizer** (web): colored token chips for OpenAI / Gemini, count-only view for Claude.
- **CLI commands:** `count`, `viz`, `redact`, `compare`.
- **Cost calculator** with cache toggle, batch toggle, hidden costs surfaced.

**Done when:** drop a PDF / config / prompt and get accurate counts and dollar estimates across all three providers in under 2 seconds for typical inputs.

### Phase 2: The wedge

- **MCP config analyzer.** Parse all four config formats, ship the curated top-50 catalog, three scenarios (cold / warm / Tool Search), CLI swap recommendations with documented savings.
- **Audit rules engine** (production-proven detections only). Lighthouse-style ranked issue list with severity, savings, confidence, citation links.
- **Claude hybrid mode (opt-in).** "Verify with Anthropic" button calls `/v1/messages/count_tokens` from the browser. Shows local-vs-API delta. SHA-256-content-keyed cache.
- **Tokenizer-drift comparator.** Same input across Claude / GPT / Gemini versions.
- **Cross-provider cost comparison UI.** Treemap of token waste by region (system / context / examples / MCP / tool results).
- **CLI commands:** `audit`, `mcp`, `cost`, `drift`.

**Done when:** the MCP analyzer is the demo we lead with at launch.

### Phase 3: Frontier

- **LLMLingua-2 in-browser preview.** `@atjsh/llmlingua-2` (TS port, runs via transformers.js / ONNX with WebGPU when available, TinyBERT or MobileBERT variants <100MB). Actual compression preview with retention indicator and "lossy" warning. Aggressive r=0.2 is gated behind a "show me anyway" advanced toggle with the pre-registered RCT caveat.
- **Model2Vec static embeddings** (opt-in upgrade) for semantic dedup detection ("the cat sat on the mat" / "a feline rested on the rug" case).
- **Experimental detections shipped** with explicit confidence intervals and badging.
- **Format efficiency previews:** HTML to Markdown, JSON to TOON for uniform arrays (with the indentation-drift caveat), YAML compaction.
- **System-prompt and few-shot pruning analyzer.**

### Phase 4: Distribution and community

- **Open source release.** MIT license, public README, contribution guide.
- **GitHub Action / Bun script** for SDLC pipelines: `tolkin audit` runs on PR diffs, posts a comment with severity-tagged findings.
- **Community-curated MCP catalog.** Move the bundled catalog to a separate repo where vendors and users can PR additions.
- **Plug-in interface for additional providers.** Open-weight models via Hugging Face tokenizer wiring.
- **Hosted demo** at a Tolkin subdomain (still 100% client-side; the host is just CDN).

## 14. Tradeoffs and risks

1. **Claude offline accuracy.** `bpe-lite` is the current SOTA approximator at 84% within 10%, but Arabic / emoji / heavy symbol text can hit 25% error. Mitigation: confidence band UI, opt-in hybrid verification, tokenizer version pinning (label the bpe-lite version and benchmark date in the UI), automated weekly diff against `count_tokens` on a fixture corpus to catch drift.
2. **Anthropic tokenizer is closed.** If Anthropic ships a new tokenizer silently (as they did with Opus 4.7), every approximator drifts. Mitigation: "tokenizer last verified" badge on every Claude count; CI job that re-runs the fixture corpus against `count_tokens` weekly and posts a regression alert.
3. **MCP catalog maintenance burden.** Top-50 servers drift every month. Mitigation: catalog is data, not code; ship a CLI subcommand `tolkin mcp refresh-catalog` that fetches from a curated GitHub repo; allow user-supplied `tools/list` paste as fallback.
4. **Tokenizer-drift comparator as a feature.** Anthropic may not love a tool that headlines "Opus 4.7 quietly costs 35% more." This is a brand risk but a user-trust win; the right framing is descriptive ("here is what changed") rather than accusatory.
5. **Recommendation aggressiveness vs trust.** The pre-registered RCT shows aggressive compression can backfire on total cost. Mitigation: input-token-bounded claims everywhere, output caveat surfaced on every recommendation, experimental detections badged distinctly.
6. **Bundle size on mobile.** Even with aggressive lazy-loading, OCR + Gemma tokenizer + multiple grammars can push past 5MB on Mobile Safari (250MB tab limit). Mitigation: degradation modes ("OCR unavailable on mobile, drop a text PDF instead"); detect Mobile Safari and disable the heaviest opt-in features by default.
7. **WASM bundle on web.** The Rust core compiled to WASM is the right architecture, but bundle size and instantiation latency must be measured. Mitigation: code-split the WASM at the module boundary (rules engine, MCP, cost, redactor each as their own WASM artifact if needed); benchmark on first load.
8. **CLI npm distribution supply-chain risk.** Auto-downloading a binary on `postinstall` is the same pattern esbuild / swc / biome use; it's also a vector for tampering. Mitigation: SHA-256 checksums in the npm package, sigstore signatures on the GitHub releases (when sigstore matures for native binaries).
9. **Naming.** "Tolkin" is cleaner than Tokenist but should be verified for domain (`tolkin.dev`, `tolkin.ai`, `tolkin.io`), npm scope (`@tolkin`), GitHub org availability before any public posture.

## 15. Open questions for future sessions

- **Which Rust PDF library?** `pdf-extract` is pure-Rust but limited; `mupdf` Rust binding is FFI-heavy but battle-tested. Decide based on a benchmark of typical input sizes.
- **WASM vs JS-only for the redactor in the browser.** WASM gives single-source-of-truth, but a pure-JS regex pass is simpler and probably fast enough. Measure before committing.
- **Bundle the Gemma tokenizer model in the web app, or fetch from CDN?** ~4 MB model file. Bundling makes the offline story stronger; fetching makes the initial bundle lighter. Probably: fetch from same origin (Vercel static asset) with `Cache-Control: public, immutable, max-age=31536000` to get one-time download + perpetual cache.
- **Catalog format for MCP servers.** TOML, JSON, or a small DSL? Lean toward JSON (easy to PR, easy to lint).
- **Pricing-table update cadence.** Manual quarterly refresh, or a scheduled GitHub Action that scrapes provider pricing pages and opens a PR with the diff?
- **Hosted demo deployment.** Subdomain of the main site, or a separate Vercel project? Cleaner separation supports the open-source extraction.
- **License compatibility for `mupdf` Rust binding** (AGPL or commercial) if we choose it for the CLI. Likely `pdf-extract` is the right choice for an MIT-licensed project even if extraction quality is worse.

## 16. References

Tokenizers and accuracy:
- [gpt-tokenizer (niieani)](https://github.com/niieani/gpt-tokenizer)
- [tiktoken-rs](https://github.com/zurawiki/tiktoken-rs)
- [Anthropic count_tokens API docs](https://platform.claude.com/docs/en/api/messages-count-tokens)
- [Simon Willison: anthropic-dangerous-direct-browser-access](https://simonwillison.net/2024/Aug/23/anthropic-dangerous-direct-browser-access/)
- [bpe-lite benchmark (DEV.to)](https://dev.to/jerown/anthropic-never-released-their-tokenizer-heres-what-we-found-testing-the-alternatives-b05)
- [@huggingface/transformers v4](https://huggingface.co/blog/transformersjs-v4)
- [Gemini countTokens API](https://ai.google.dev/api/tokens)
- [Anthropic Opus 4.7 tokenizer change announcement](https://platform.claude.com/docs/en/about-claude/pricing)

Compression literature:
- [LLMLingua-2](https://arxiv.org/abs/2403.12968)
- [LLMLingua](https://arxiv.org/abs/2310.05736)
- [LongLLMLingua](https://arxiv.org/abs/2310.06839)
- [Selective Context](https://github.com/liyucheng09/Selective_Context)
- [RECOMP](https://arxiv.org/abs/2310.04408)
- [Pre-registered RCT on prompt compression (Mar 2026)](https://arxiv.org/abs/2603.23525)
- [Prompt-defect taxonomy](https://arxiv.org/abs/2509.14404)
- [Hidden cost of readability](https://arxiv.org/abs/2508.13666)
- [Caveman compression](https://github.com/wilpel/caveman-compression)
- [@atjsh/llmlingua-2 (JS/TS port)](https://www.npmjs.com/package/@atjsh/llmlingua-2)

MCP analysis:
- [Anthropic Code Execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp)
- [Anthropic Effective Context Engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [Scalekit MCP vs CLI benchmark](https://github.com/scalekit-inc/mcp-vs-cli-benchmark)
- [MCP context tax (mmntm)](https://www.mmntm.net/articles/mcp-context-tax)
- [Async Let: MCP token reporting](https://www.async-let.com/posts/claude-code-mcp-token-reporting/)
- [Tool Search Tool defer-loading](https://medium.com/@DebaA/anthropic-just-shipped-the-fix-for-tool-definition-bloat-77464c8dbec9)
- [MindStudio: reduce token usage via MCP optimization](https://www.mindstudio.ai/blog/reduce-token-usage-ai-agents-mcp-optimization)
- [notion-slim (52% reduction)](https://github.com/mcpslim/notion-slim)
- [The MCP context bloat at enterprise scale](https://agentmarketcap.ai/blog/2026/04/08/mcp-context-bloat-enterprise-scale-tool-definitions-agent-context-budget)

Secret detection:
- [secretlint](https://github.com/secretlint/secretlint)
- [trufflesecurity/trufflehog detectors](https://github.com/trufflesecurity/trufflehog/tree/main/pkg/detectors)
- [gitleaks default config](https://github.com/gitleaks/gitleaks/blob/master/config/gitleaks.toml)
- [GitHub supported secret scanning patterns](https://docs.github.com/en/code-security/reference/secret-security/supported-secret-scanning-patterns)
- [Yelp detect-secrets](https://github.com/Yelp/detect-secrets)
- [Secrets-Patterns-DB](https://github.com/mazen160/secrets-patterns-db)

Pricing and economics:
- [Anthropic pricing](https://platform.claude.com/docs/en/about-claude/pricing)
- [Anthropic prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [OpenAI API pricing](https://openai.com/api/pricing/)
- [OpenAI Prompt Caching 201](https://developers.openai.com/cookbook/examples/prompt_caching_201)
- [Gemini API pricing](https://ai.google.dev/gemini-api/docs/pricing)
- [Gemini context caching](https://ai.google.dev/gemini-api/docs/caching)
- [GitHub Blog: token efficiency in agentic workflows](https://github.blog/ai-and-ml/github-copilot/improving-token-efficiency-in-github-agentic-workflows/)
- [TrueFoundry: agentic token explosion in CI/CD](https://www.truefoundry.com/blog/the-agentic-token-explosion-in-ci-cd)

File parsing and stack:
- [pdfjs-dist](https://www.npmjs.com/package/pdfjs-dist)
- [mammoth.js](https://github.com/mwilliamson/mammoth.js)
- [unified / remark](https://github.com/remarkjs/remark)
- [eemeli/yaml](https://github.com/eemeli/yaml)
- [smol-toml](https://github.com/squirrelchat/smol-toml)
- [jsonc-parser (Microsoft)](https://github.com/microsoft/node-jsonc-parser)
- [web-tree-sitter](https://github.com/tree-sitter/tree-sitter)
- [calamine (Rust xlsx)](https://github.com/tafia/calamine)
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)

UX inspiration:
- [Tiktokenizer (dqbd)](https://github.com/dqbd/tiktokenizer)
- [Claude Token Counter (Simon Willison)](https://simonwillison.net/2026/apr/20/claude-token-counts/)
- [Lighthouse](https://developer.chrome.com/docs/lighthouse)
- [webpack-bundle-analyzer](https://github.com/webpack-contrib/webpack-bundle-analyzer)
- [AWS Pricing Calculator](https://calculator.aws/)
- [Website Carbon](https://www.websitecarbon.com/)
- [Brave Leo AI](https://brave.com/blog/ai-browsing/)
- [WebLLM](https://webllm.mlc.ai/)
