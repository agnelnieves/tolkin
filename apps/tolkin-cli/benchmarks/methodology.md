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
