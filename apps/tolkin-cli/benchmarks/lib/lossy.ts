// Lossy track: LLMLingua-2 (the same library the web preview uses) at target
// ratios 0.7 / 0.5 / 0.33 against each prose fixture. Quality is NOT scored
// in this track; the schema's quality_scoring field stays
// { scored: false, method: "BYOK extraction-QA harness, off by default" } and
// every case carries the RCT caveat at the track level.
//
// Note on model fetch: the LLMLingua-2 ONNX weights (atjsh/llmlingua-2-js-
// tinybert-meetingbank) download from the Hugging Face Hub on first run.
// This is dev-tooling for the harness and is documented in the case notes
// and methodology.

import { readFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { countTokens, REPO_ROOT } from "./cli";
import type { ExternalComparison, LossyCase } from "./types";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE_ROOT = resolve(HERE, "..", "fixtures", "lossy");

const TARGET_RATIOS = [0.7, 0.5, 0.33] as const;
const RUNS = 3; // model inference once; tokenization three times per side
// Bun's linker layout decides where workspace deps land (hoisted to the root
// node_modules locally, isolated under the owning package in CI). The library
// belongs to the web app's package.json, so resolve from that package's
// context when the bare specifier misses.
async function importFromWebWorkspace(specifier: string): Promise<any> {
  try {
    return await import(specifier);
  } catch {
    const parent = resolve(REPO_ROOT, "apps/tolkin-web", "package.json");
    const resolved = Bun.resolveSync(specifier, dirname(parent));
    return await import(resolved);
  }
}

const MODEL_ID = "atjsh/llmlingua-2-js-tinybert-meetingbank";
const LLMLINGUA2_CONFIG = { max_batch_size: 50, max_force_token: 100, max_seq_length: 312 };

interface CaseSpec {
  id: string;
  filename: string;
}

const CASES: CaseSpec[] = [
  { id: "lossy/technical-explainer", filename: "technical-explainer.txt" },
  { id: "lossy/meeting-notes", filename: "meeting-notes.txt" },
  { id: "lossy/verbose-instructions", filename: "verbose-instructions.txt" },
];

type Compressor = {
  compress(text: string, options: { rate: number }): Promise<string>;
};

let compressorPromise: Promise<Compressor> | null = null;

async function loadCompressor(): Promise<Compressor> {
  // Imports resolve via the workspace's hoisted node_modules at the repo root
  // when bun is run from the repo. The web app already pins these versions;
  // the harness reuses them so the comparison stays apples-to-apples with the
  // /compress panel in the browser.
  const { LLMLingua2 } = await importFromWebWorkspace("@atjsh/llmlingua-2");
  const { AutoConfig, AutoTokenizer, BertForTokenClassification, env } =
    await importFromWebWorkspace("@huggingface/transformers");
  const { encode } = await importFromWebWorkspace("gpt-tokenizer/encoding/o200k_base");
  const { makeLLMLingua2TokenizerAdapter } = await import("./tokenizer-adapter");

  // Node/Bun has no localStorage to cache models, so transformers.js writes
  // to ~/.cache/huggingface by default. allowLocalModels=true lets it reuse
  // a cached download on the second run.
  env.allowLocalModels = true;
  env.allowRemoteModels = true;

  const tjsConfig = { device: "auto", dtype: "fp32" } as const;
  const config = await AutoConfig.from_pretrained(MODEL_ID);
  const withConfig = { config: { ...config, "transformers.js_config": tjsConfig } };
  const tokenizer = await AutoTokenizer.from_pretrained(MODEL_ID, withConfig);
  const model = await BertForTokenClassification.from_pretrained(MODEL_ID, withConfig);

  return new LLMLingua2.PromptCompressor(
    model,
    makeLLMLingua2TokenizerAdapter(tokenizer) as never,
    LLMLingua2.get_pure_tokens_bert_base_multilingual_cased,
    LLMLingua2.is_begin_of_new_word_bert_base_multilingual_cased,
    { encode: (text: string) => encode(text) } as never,
    LLMLINGUA2_CONFIG,
    () => {},
  );
}

function ensureCompressor(): Promise<Compressor> {
  if (!compressorPromise) {
    compressorPromise = loadCompressor().catch((e: unknown) => {
      compressorPromise = null;
      throw e;
    });
  }
  return compressorPromise;
}

async function tokenize3x(text: string, label: string): Promise<number> {
  // Tokenization is deterministic in tolkin's o200k path; we count three
  // times to enforce the contract and surface drift loudly.
  let last = -1;
  for (let i = 0; i < RUNS; i++) {
    const n = await countTokens(text, "openai");
    if (i > 0 && n !== last) {
      throw new Error(`non-deterministic count for ${label}: got ${last} then ${n}`);
    }
    last = n;
  }
  return last;
}

function round4(n: number): number {
  return Math.round(n * 10_000) / 10_000;
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export async function runLossy(): Promise<LossyCase[]> {
  const cases: LossyCase[] = [];
  const compressor = await ensureCompressor();
  for (const spec of CASES) {
    const fixture = resolve(FIXTURE_ROOT, spec.filename);
    const before = await readFile(fixture, "utf8");
    const beforeTokens = await tokenize3x(before, `${spec.id}/before`);
    for (const rate of TARGET_RATIOS) {
      const after = await compressor.compress(before, { rate });
      const afterTokens = await tokenize3x(after, `${spec.id}@${rate}/after`);
      const achievedRatio = beforeTokens === 0 ? 0 : afterTokens / beforeTokens;
      const savingsPct = beforeTokens === 0 ? 0 : (1 - afterTokens / beforeTokens) * 100;
      cases.push({
        id: `${spec.id}@rate=${rate}`,
        fixture: relative(REPO_ROOT, fixture),
        technique: "LLMLingua-2 (atjsh/llmlingua-2-js-tinybert-meetingbank)",
        tokenizer: "o200k_base (exact)",
        target_ratio: rate,
        before_tokens: beforeTokens,
        after_tokens: afterTokens,
        achieved_ratio: round4(achievedRatio),
        savings_pct: round2(savingsPct),
        notes:
          "Quality not scored (track-level rct_caveat applies). Model weights fetched once from the Hugging Face Hub (atjsh/llmlingua-2-js-tinybert-meetingbank); subsequent runs reuse the on-disk cache, so the count is the only run-to-run variable.",
      });
    }
  }
  return cases;
}

export async function runLossyComparisons(): Promise<ExternalComparison[]> {
  const comparisons: ExternalComparison[] = [];

  // caveman-shrink: pure prose compressor, measured against the same prose
  // fixtures at their natural rate (it has no rate knob; the rule set is fixed).
  const compressJsPath = resolve(HERE, "..", "external", "caveman-shrink-compress.js");
  const mod = (await import(compressJsPath)) as {
    compress: (text: string) => { compressed: string; before: number; after: number };
  };
  let totalBefore = 0;
  let totalAfter = 0;
  for (const spec of CASES) {
    const fixture = resolve(FIXTURE_ROOT, spec.filename);
    const text = await readFile(fixture, "utf8");
    const { compressed } = mod.compress(text);
    const before = await countTokens(text, "openai");
    const after = await countTokens(compressed, "openai");
    totalBefore += before;
    totalAfter += after;
  }
  const savingsPct = totalBefore === 0 ? 0 : (1 - totalAfter / totalBefore) * 100;
  comparisons.push({
    name: "caveman-shrink (JuliusBrussee/caveman, src/mcp-servers/caveman-shrink/compress.js)",
    status: "measured",
    reason:
      "MIT-licensed, pure-Node prose compressor with no rate knob. Vendored at benchmarks/external/caveman-shrink-compress.js and run on the same three lossy fixtures the LLMLingua-2 cases use. Sum of before/after tokens reported across the three prose fixtures (o200k_base).",
    before_tokens: totalBefore,
    after_tokens: totalAfter,
    savings_pct: round2(savingsPct),
    notes:
      "Single fixed-rule pass per fixture; no model, no API key, no training data. The methodology section explains why this number is not directly comparable to LLMLingua-2's rate-targeted runs.",
  });

  comparisons.push({
    name: "wilpel/caveman-compression (NLP method)",
    status: "not-runnable-headless",
    reason:
      "MIT-licensed and exposes a Python CLI (caveman_compress_nlp.py) that runs offline, but it requires a Python virtual environment plus the spaCy en_core_web_sm model (~50 MB) which this bun-only harness does not provision. Comparable measurements can be added by running caveman_compress_nlp.py on the lossy fixtures externally and amending this file.",
  });

  return comparisons;
}
