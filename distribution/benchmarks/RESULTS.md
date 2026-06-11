# Tolkin benchmark results

Numbers must be honest or absent. Each row names its fixture and tokenizer. Anything not measured is labeled, never silently estimated.

## Methodology

## Why this benchmark exists

Token-savings claims usually arrive as a single number, and that number can come from three different places: output-side advice (instructions that make a model answer more tersely), input-side rewriting (compressing the text before it is sent), and structural or configuration measurement (removing tokens from files and tool definitions without changing meaning). These are different operations with different risks and different baselines. Collapsing them into one headline percent is how this space lies to itself.

The cautionary example is public. A viral output-side skill (JuliusBrussee/caveman) claimed a 65 percent average output reduction. An independent 72-run benchmark (Guzik, https://dev.to/jakguzik/i-benchmarked-the-viral-caveman-prompt-to-save-llm-tokens-then-my-6-line-version-beat-it-2o81) measured 9-21 percent output token reduction against a plain no-brevity-instruction baseline. The gap was not fabrication; it was baseline choice. The lesson is baselines, not villains, and this benchmark is designed so the baseline question can never be left open.

The design answer is separation. Every number below belongs to exactly one of three tracks (structural, configuration, lossy), each with its own fidelity contract, and numbers are never blended across tracks into a combined figure.

## The rules

**Declared baselines.** Every comparison states exactly what the before is: a named fixture in the repository, byte for byte, never an implied or reconstructed one.

**Real tokenizer counts.** Each number names the tokenizer that produced it: o200k_base counts are exact, while Anthropic counts via the cl100k proxy are labeled estimates, carry a plus or minus 10 percent band, and never appear in headline rows.

**Injection overhead counted.** Any technique that requires runtime prompt instructions has those instruction tokens charged against its savings.

**N runs with variance.** Every case runs N times and publishes min and max; deterministic transforms must show zero variance, and the harness fails loudly if one does not.

**Date-stamped pricing.** Every dollar figure carries the PRICES_OBSERVED date it was computed under, because prices move and an undated cost cannot be checked.

**Everything in the repository.** Fixtures, runner, and results are versioned together, and a single command regenerates all of them.

**Input-token bounded.** A pre-registered randomized trial (arXiv 2603.23525) found that moderate compression (ratio 0.5) cut total cost 27.9 percent while aggressive compression (ratio 0.2) increased total cost 1.8 percent because outputs grew; every number here is therefore input-side only and says so wherever it appears.

## Track 1: structural (lossless)

This track measures transforms that cannot change meaning: minification, exact-duplicate removal, and format transforms, applied to fixtures shaped like real working material (pretty-printed JSON configuration, HTML documents, prompts with repeated boilerplate, stack-trace logs). Because the transforms are lossless by construction, there is no quality question to answer, and the before and after counts are exact properties of the fixtures under the named tokenizer. This is measurement, not estimation.

## Track 2: configuration (MCP)

Every MCP server in a client configuration contributes its tool definitions to every request, whether or not the conversation uses them. This track counts that weight against a catalog of real, public server manifests: each fixture is a vendored tools/list output captured live from a named server version (provenance, capture commands, and license texts live next to the fixtures in `fixtures/configuration/manifests/`), tokenized through the real CLI path with o200k_base (exact). Every tool is canonicalized to compact `{name, description, input_schema}` JSON before counting, so the numbers are reproducible byte for byte; a specific client's wire bytes can differ by a few tokens, and that caveat ships in the analyzer output itself. Cold is the tokenized weight of the tool definitions with no provider price multipliers. Swap deltas (replacing a server with a CLI equivalent) derive from the tokenized cold. Slim deltas are the measured difference between two tokenized manifests (the server captured with and without its slim setting) where a slim capture exists; a slim percentage estimate is never applied to a measured cold, because that would blend bases. The tolkin analyzer's curated catalog estimates still exist as a product surface for configs without manifests, and they are labeled as estimates there; they no longer produce benchmark rows. The three earlier config-shape fixtures are retired from this track for exactly that reason, and config-shape parsing is covered by the analyzer's unit tests instead. Only the configuration changes; conversation content is untouched. Comparable middleware (caveman-shrink) runs on the same manifests, headlessly, with measured before and after counts. External claims that can be partially measured (notion-slim claims 52 percent against the full Notion MCP) ship the half that is publicly obtainable (the full Notion manifest measured here) and a not-runnable-headless row with the precise reason the other half cannot be obtained from this platform (notion-slim's npm tarball ships only Windows binaries despite claiming cross-platform support).

## Track 3: lossy (compression)

The caveat leads this track: the randomized trial cited above showed that aggressive input compression can grow outputs enough to increase total cost, so no lossy result here is a total-cost claim. The technique measured is LLMLingua-2 at declared target ratios, and every case publishes both the target ratio and the achieved ratio. Quality scoring exists as an extraction-QA harness (questions with verifiable answers, checked automatically) implemented in `lib/scoring.ts` against author-declared QA fixtures at `fixtures/lossy/scoring/<id>.qa.json`; it runs only with the user's own API key and only when the `--score-quality` flag is passed, so it is off by default. Published results state whether scoring ran, and no quality claim is made without it. Comparable public rewriters (wilpel/caveman-compression, repomix --compress, cavemem compress) run on the same fixtures under the same rules where they can be run headlessly. The Notion-OpenAPI server's `--compress` flag (repomix) is a code-pack compressor whose published "about 70 percent" figure is measured against an undeclared corpus; the harness publishes the achieved ratio on a self-authored declared corpus next to the upstream claim with both denominators stated. cavemem compress is measured the same way against the lossy track's prose fixtures.

## What this benchmark does not claim

No total-cost claims: every figure is input tokens, and total cost depends on output behavior this harness does not measure. No output-side claims: nothing here measures how a model answers. No quality claims without the scored flag: a compression ratio is not a quality result, and results state explicitly whether the extraction-QA harness ran. No numeric comparisons against tools that cannot be run headlessly on the same inputs: those tools appear in the results with a status and a reason instead of numbers, because a figure produced under different conditions is not a comparison.

## Reproducing the results

Clone the repository, run `bun install` at the root, build the release `tolkin` binary (`cargo build --release` in `apps/tolkin-cli`), and run the benchmark harness in `apps/tolkin-cli/benchmarks/`; it regenerates `results.json` and `RESULTS.md` from the fixtures in the same directory. Then run it a second time and diff the two outputs: the only field permitted to differ between runs on the same tree is the `generated_at` timestamp. Anything else is a determinism failure, and the harness treats it as one.

### One-command scored run (BYOK)

To publish a scored lossy run (a `scored=true` track with per-case extraction accuracy), the owner runs the same harness with one extra flag and one extra env var:

```
ANTHROPIC_API_KEY=<your key> bun apps/tolkin-cli/benchmarks/run.ts --score-quality
```

What this does, step by step:

1. The harness builds every track exactly the way `bun apps/tolkin-cli/benchmarks/run.ts` does (structural, configuration, lossy compression).
2. For every lossy case, the runner captures the compressed text produced by LLMLingua-2 and passes it, plus each question from the matching `fixtures/lossy/scoring/<id>.qa.json`, to Claude via the official messages endpoint.
3. The grader is purely deterministic: `lib/scoring.ts` lowercases and whitespace-normalizes both the model's reply and the author-declared accepted answers, then accepts the reply when it equals or contains an accepted answer. There is no LLM-as-judge.
4. The track-level `quality_scoring` field becomes `{ scored: true, grader_model: "claude-opus-4-7", anthropic_version: "2023-06-01", method: ... }` and every lossy case gains a `scored` object with `questions_total`, `questions_correct`, `accuracy`, and the full per-question answers.
5. `--check-determinism` is mutually exclusive with `--score-quality`: live Claude calls are not byte-for-byte deterministic, and the contract is that the deterministic mode runs without scoring and scoring is a separate owner action.

Override the grader model via the env var `TOLKIN_BENCH_SCORE_MODEL=<id>` (default `claude-opus-4-7`). The QA fixtures are author-declared and ship in the repository; extending or correcting an accepted-answers list is a fixture-edit and does not require rerunning the API call to verify (a re-grade against the recorded `reply` is enough).

## Structural track (lossless)

Exact before / after token counts via the tolkin CLI's o200k_base tokenizer. Injection overhead is 0 because none of these transforms requires a runtime prompt instruction.

| Case | Fixture | Technique | Tokenizer | Before | After | Saved | Saved % | Injection | Runs |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| structural/json-minify | `apps/tolkin-cli/benchmarks/fixtures/structural/app-config.json` | JSON.parse / JSON.stringify (compact) | o200k_base (exact) | 1,305 | 882 | 423 | 32.41% | 0 | 3 |
| structural/html-to-markdown | `apps/tolkin-cli/benchmarks/fixtures/structural/marketing-page.html` | HTML -> Markdown (audit preview rules) | o200k_base (exact) | 1,111 | 539 | 572 | 51.49% | 0 | 3 |
| structural/paragraph-dedup | `apps/tolkin-cli/benchmarks/fixtures/structural/duplicated-paragraphs.txt` | exact-text paragraph dedup (first-occurrence kept) | o200k_base (exact) | 793 | 441 | 352 | 44.39% | 0 | 3 |
| structural/stack-trace-dedup | `apps/tolkin-cli/benchmarks/fixtures/structural/stack-trace.log` | stack-trace dump dedup (replace identical body with marker) | o200k_base (exact) | 2,022 | 1,442 | 580 | 28.68% | 0 | 3 |

- **structural/json-minify**. Strict JSON in, compact JSON out via JSON.parse(text); JSON.stringify(value). Lossless by construction (no key reorder beyond V8 insertion order). Mirrors tolkin-core::format::json_minify.
- **structural/html-to-markdown**. Reproduces tolkin-core::format::html_to_markdown rules in TS so the artifact is not capped at the audit preview's 8 KB limit. Attribute data and layout semantics are dropped (near-lossless).
- **structural/paragraph-dedup**. Split on blank lines; drop blocks whose trimmed text matches a prior block byte-for-byte. Lossless when duplicates are verbatim, which is the case the fixture exercises.
- **structural/stack-trace-dedup**. Split on JVM error-header lines; when two dumps share an identical post-header body, replace the second body with a one-line marker pointing at the first dump's timestamp. Lossless for the message text and structure preserved.

## Configuration track (lossless-configuration)

Each fixture is a vendored, real, public server tools/list manifest (provenance in `fixtures/configuration/manifests/README.md`), tokenized through the real CLI path (`tolkin mcp <manifest> --json`): every tool is canonicalized to compact {name, description, input_schema} JSON and counted with o200k_base (exact). Cold is the tokenized weight of the tool definitions; no provider price multipliers are folded in. Swap savings derive from the tokenized cold; slim savings are the measured difference of two tokenized manifests where a slim capture exists, otherwise 0 (never an estimate applied to a measured base).

| Case | Fixture | Basis | Tokenizer | Tools | Cold | Swap saves | Slim saves | % of 200K window |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| configuration/server-filesystem | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/server-filesystem.tools.json` | tokenized-manifest | o200k_base (exact) | 14 | 1,641 | 1,641 | 0 | 0.82% |
| configuration/server-memory | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/server-memory.tools.json` | tokenized-manifest | o200k_base (exact) | 9 | 904 | 0 | 0 | 0.45% |
| configuration/server-everything | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/server-everything.tools.json` | tokenized-manifest | o200k_base (exact) | 13 | 1,126 | 0 | 0 | 0.56% |
| configuration/github-mcp-server | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/github-mcp-server.tools.json` | tokenized-manifest | o200k_base (exact) | 43 | 8,175 | 8,175 | 3,222 | 4.09% |
| configuration/github-mcp-server-slim | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/github-mcp-server.slim.tools.json` | tokenized-manifest | o200k_base (exact) | 27 | 4,953 | 4,953 | 0 | 2.48% |
| configuration/notion-mcp-server | `apps/tolkin-cli/benchmarks/fixtures/configuration/manifests/notion-mcp-server.tools.json` | tokenized-manifest | o200k_base (exact) | 22 | 15,698 | 0 | 0 | 7.85% |

Manifest provenance (full detail and license texts in the manifests directory):

- **configuration/server-filesystem**: https://github.com/modelcontextprotocol/servers (src/filesystem), npm @modelcontextprotocol/server-filesystem@2026.1.14; server version secure-filesystem-server 0.2.0 (package 2026.1.14); captured 2026-06-10; license MIT per npm package.json (repo LICENSE records the MIT to Apache-2.0 transition); vendored at fixtures/configuration/manifests/LICENSE-modelcontextprotocol-servers.
- **configuration/server-memory**: https://github.com/modelcontextprotocol/servers (src/memory), npm @modelcontextprotocol/server-memory@2026.1.26; server version memory-server 0.6.3 (package 2026.1.26); captured 2026-06-10; license MIT per npm package.json (repo LICENSE records the MIT to Apache-2.0 transition); vendored at fixtures/configuration/manifests/LICENSE-modelcontextprotocol-servers.
- **configuration/server-everything**: https://github.com/modelcontextprotocol/servers (src/everything), npm @modelcontextprotocol/server-everything@2026.1.26; server version mcp-servers/everything 2.0.0 (package 2026.1.26); captured 2026-06-10; license MIT per npm package.json (repo LICENSE records the MIT to Apache-2.0 transition); vendored at fixtures/configuration/manifests/LICENSE-modelcontextprotocol-servers.
- **configuration/github-mcp-server**: https://github.com/github/github-mcp-server, release v1.2.0 (Darwin arm64 asset); server version github-mcp-server 1.2.0, default toolsets; captured 2026-06-10; license MIT; vendored at fixtures/configuration/manifests/LICENSE-github-mcp-server.
- **configuration/github-mcp-server-slim**: https://github.com/github/github-mcp-server, release v1.2.0 (Darwin arm64 asset); server version github-mcp-server 1.2.0, GITHUB_TOOLSETS=repos,issues; captured 2026-06-10; license MIT; vendored at fixtures/configuration/manifests/LICENSE-github-mcp-server.
- **configuration/notion-mcp-server**: https://github.com/makenotion/notion-mcp-server, npm @notionhq/notion-mcp-server@2.2.1; server version notion-mcp-server 2.2.1, default toolset (no env vars set); captured 2026-06-10; license MIT (Copyright (c) 2025 Notion Labs, Inc.); vendored at fixtures/configuration/manifests/LICENSE-notion-mcp-server.

### Configuration comparisons

| Name | Status | Before | After | Saved % | Reason |
| --- | --- | ---: | ---: | ---: | --- |
| caveman-shrink (JuliusBrussee/caveman, src/mcp-servers/caveman-shrink/compress.js) | measured | 27,544 | 27,024 | 1.89% | Runnable headlessly (MIT, pure Node, vendored). Its compressDescriptionsInPlace rewrites description fields inside tools/list responses; applied to the five primary vendored manifests (the four reference servers plus the full Notion manifest; github slim excluded to avoid double-counting one server) and re-tokenized through the same tolkin CLI path (o200k_base). |
| wilpel/caveman-compression (NLP method) | not-runnable-headless | n/a | n/a | n/a | MIT-licensed and exposes a Python CLI (caveman_compress_nlp.py) that runs offline, but it requires a Python virtual environment plus the spaCy en_core_web_sm model (~50 MB) which this bun-only harness does not provision. Comparable measurements can be added by running caveman_compress_nlp.py on the same manifest descriptions externally and amending this file. |
| notion-slim (mcpslim/notion-slim, npm notion-slim@2.0.0-slim.1.10) | not-runnable-headless | n/a | n/a | n/a | MIT-licensed, but the npm tarball at notion-slim@2.0.0-slim.1.10 only ships Windows binaries (bin/mcpslim.exe and bin/mcpslim-windows-x64.exe); index.js references mcpslim-darwin-arm64 and mcpslim-linux-x64 paths that are not in the tarball, the GitHub repository has no published releases, and the transformation runs inside a closed-source binary. This bun-on-macOS/Linux harness cannot exercise the slim transformation headlessly, and substituting a hand-built approximation of the recipe (recipes/notion.json grouping the 22 upstream tools into 10) is forbidden by the methodology. To verify externally: install notion-slim@<version> on Windows, capture its tools/list with the same JSON-RPC handshake as fixtures/configuration/manifests/capture.ts, vendor it next to notion-mcp-server.tools.json, add it to the configuration cases, and re-run the harness; the achieved saved percent will land next to the catalog's about-52-percent figure. |

- **configuration/server-filesystem**. Reference filesystem server, captured live over stdio. Catalog entry says replace with shell builtins, so the swap savings equal the measured cold. The catalog's representative estimate for this server is 2,000 tokens; the measured manifest supersedes it.
- **configuration/server-memory**. Reference memory server, captured live. Catalog recommendation is keep (no CLI equivalent), so swap savings are 0 by design. The catalog's representative estimate for this server is 3,000 tokens; the measured manifest supersedes it.
- **configuration/server-everything**. Reference protocol-exercise server. NOT in tolkin's curated catalog: this row exists because manifest measurement covers servers the catalog has never seen, which was the catalog's blind spot. No swap or slim recommendation, so those cells are 0.
- **configuration/github-mcp-server**. GitHub official server at its v1.2.0 defaults (43 tools). The catalog's representative figure is 40,000 tokens for the all-toolsets era (90 to 162 tools, externally reported 26-55K); the v1.2.0 default registration measures far smaller, which is exactly the staleness manifest measurement exists to correct. Slim savings are measured, not estimated: this cold minus the tokenized GITHUB_TOOLSETS=repos,issues manifest below.
- **configuration/github-mcp-server-slim**. The same binary captured with the exact slim snippet tolkin recommends (GITHUB_TOOLSETS=repos,issues; setting the env explicitly gates off the other default toolsets, including context, so this fixture is repos+issues only and the issues set registers get_label per upstream labels.go). This row IS the slim profile, so its own slim cell is 0.
- **configuration/notion-mcp-server**. Notion's official MCP server at v2.2.1. The tools are generated from scripts/notion-openapi.json at boot; no NOTION_TOKEN is required to respond to tools/list (the manifest is derived from the OpenAPI spec, no api.notion.com call happens during the initialize/tools-list handshake). Catalog representative cold is 26,000 tokens for the 21-tool era; the measured manifest (22 tools at o200k_base) supersedes that estimate. The catalog's `notion-slim, about 52% fewer tokens` note cites a fork whose npm publication only ships Windows binaries, so the slim side of that claim ships as `not-runnable-headless` in the comparisons table below.

## Lossy track

> Aggressive compression can increase total cost because outputs grow; every number here is input-token bounded.

Quality scoring: scored=false, method=BYOK extraction-QA harness, off by default (reason: --score-quality flag not passed (default off)). Each ratio runs the compressor once and tokenizes the output three times to enforce the deterministic-count contract.

| Case | Fixture | Technique | Tokenizer | Target | Before | After | Achieved | Saved % |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| lossy/technical-explainer@rate=0.7 | `apps/tolkin-cli/benchmarks/fixtures/lossy/technical-explainer.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.70 | 809 | 543 | 0.6712 | 32.88% |
| lossy/technical-explainer@rate=0.5 | `apps/tolkin-cli/benchmarks/fixtures/lossy/technical-explainer.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.50 | 809 | 388 | 0.4796 | 52.04% |
| lossy/technical-explainer@rate=0.33 | `apps/tolkin-cli/benchmarks/fixtures/lossy/technical-explainer.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.33 | 809 | 252 | 0.3115 | 68.85% |
| lossy/meeting-notes@rate=0.7 | `apps/tolkin-cli/benchmarks/fixtures/lossy/meeting-notes.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.70 | 906 | 633 | 0.6987 | 30.13% |
| lossy/meeting-notes@rate=0.5 | `apps/tolkin-cli/benchmarks/fixtures/lossy/meeting-notes.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.50 | 906 | 458 | 0.5055 | 49.45% |
| lossy/meeting-notes@rate=0.33 | `apps/tolkin-cli/benchmarks/fixtures/lossy/meeting-notes.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.33 | 906 | 312 | 0.3444 | 65.56% |
| lossy/verbose-instructions@rate=0.7 | `apps/tolkin-cli/benchmarks/fixtures/lossy/verbose-instructions.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.70 | 922 | 622 | 0.6746 | 32.54% |
| lossy/verbose-instructions@rate=0.5 | `apps/tolkin-cli/benchmarks/fixtures/lossy/verbose-instructions.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.50 | 922 | 440 | 0.4772 | 52.28% |
| lossy/verbose-instructions@rate=0.33 | `apps/tolkin-cli/benchmarks/fixtures/lossy/verbose-instructions.txt` | LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank) | o200k_base (exact) | 0.33 | 922 | 292 | 0.3167 | 68.33% |

### Lossy comparisons

| Name | Status | Before | After | Saved % | Reason |
| --- | --- | ---: | ---: | ---: | --- |
| caveman-shrink (JuliusBrussee/caveman, src/mcp-servers/caveman-shrink/compress.js) | measured | 2,637 | 2,328 | 11.72% | MIT-licensed, pure-Node prose compressor with no rate knob. Vendored at benchmarks/external/caveman-shrink-compress.js and run on the same three lossy fixtures the LLMLingua-2 cases use. Sum of before/after tokens reported across the three prose fixtures (o200k_base). |
| wilpel/caveman-compression (NLP method) | not-runnable-headless | n/a | n/a | n/a | MIT-licensed and exposes a Python CLI (caveman_compress_nlp.py) that runs offline, but it requires a Python virtual environment plus the spaCy en_core_web_sm model (~50 MB) which this bun-only harness does not provision. Comparable measurements can be added by running caveman_compress_nlp.py on the lossy fixtures externally and amending this file. |
| repomix --compress (yamadashy/repomix, npm repomix@1.14.1) | measured | 3,200 | 1,745 | 45.47% | MIT-licensed, runs headlessly with no network call once installed. Pinned via bun (apps/tolkin-cli devDependencies); LICENSE vendored at benchmarks/external/repomix-LICENSE. Packed the self-authored declared corpus at benchmarks/fixtures/lossy/repomix-corpus (auth.ts, billing.ts, email.py) twice through repomix v1.14.1, once with --compress and once without, then tokenized both XML packs through the same `tolkin count --model openai` path the other rows use. The README's about-70-percent figure has no stated basis (no benchmark file, no methodology, no fixture published with the claim); this row publishes what the harness measures alongside it. |
| cavemem compress (JuliusBrussee/cavemem, npm cavemem@0.2.1) | measured | 2,637 | 2,327 | 11.76% | MIT-licensed, runs headlessly via `cavemem compress <file>`; the subcommand is pure-JS and does not touch the sqlite store or the embedding worker. Pinned via bun (apps/tolkin-cli devDependencies); LICENSE vendored at benchmarks/external/cavemem-LICENSE. Run on the same three prose fixtures the LLMLingua-2 cases use, with token counts from the same `tolkin count --model openai` path. Upstream README claims about 46 percent for memory-store compression; basis: undisclosed (the 46 percent figure is distinct from cavemem's caveman-grammar reference of about 75 percent for prose, and from the parent caveman skill's about 65 percent for output). |

- **lossy/technical-explainer@rate=0.7**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/technical-explainer@rate=0.5**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/technical-explainer@rate=0.33**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/meeting-notes@rate=0.7**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/meeting-notes@rate=0.5**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/meeting-notes@rate=0.33**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/verbose-instructions@rate=0.7**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/verbose-instructions@rate=0.5**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.
- **lossy/verbose-instructions@rate=0.33**. Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.

---

Generated at: 2026-06-11T02:30:17.885Z. Tolkin 0.10.0. Prices observed: 2026-06. Runner: bun + tolkin CLI. OS: darwin.

To regenerate: `bun apps/tolkin-cli/benchmarks/run.ts` (from the repo root). Verify determinism with `bun apps/tolkin-cli/benchmarks/run.ts --check-determinism`.
