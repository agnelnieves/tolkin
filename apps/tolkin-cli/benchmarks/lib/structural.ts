// Structural track: lossless transforms with exact before/after token counts.
// Each case names the fixture, the technique, and the tokenizer; injection
// overhead is 0 because none of these techniques requires runtime prompt
// instructions to apply.

import { readFile } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { countTokens, REPO_ROOT } from "./cli";
import { dedupParagraphs, dedupStackTrace, htmlToMarkdown, jsonMinify } from "./transforms";
import type { StructuralCase } from "./types";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE_ROOT = resolve(HERE, "..", "fixtures", "structural");

const RUNS = 3;

async function variancedCount(
  text: string,
  caseId: string,
  side: "before" | "after",
): Promise<{ value: number; min: number; max: number }> {
  // Determinism contract: the same input bytes against the same tokenizer
  // must produce the same token count every time. We re-run the count
  // RUNS times and fail loudly if the tokenizer disagrees with itself.
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  let last = -1;
  for (let i = 0; i < RUNS; i++) {
    const n = await countTokens(text, "openai");
    if (n < min) min = n;
    if (n > max) max = n;
    last = n;
  }
  if (min !== max) {
    throw new Error(
      `non-deterministic token count for ${caseId}/${side}: min=${min} max=${max} (tokenizer disagreed with itself across ${RUNS} runs)`,
    );
  }
  return { value: last, min, max };
}

function rel(absPath: string): string {
  return relative(REPO_ROOT, absPath);
}

async function runCase(args: {
  id: string;
  fixturePath: string;
  technique: string;
  transform: (text: string) => string;
  notes: string;
}): Promise<StructuralCase> {
  const before = await readFile(args.fixturePath, "utf8");
  const after = args.transform(before);
  const beforeCount = await variancedCount(before, args.id, "before");
  const afterCount = await variancedCount(after, args.id, "after");
  const savings = beforeCount.value - afterCount.value;
  const pct = beforeCount.value === 0 ? 0 : (savings / beforeCount.value) * 100;
  return {
    id: args.id,
    fixture: rel(args.fixturePath),
    technique: args.technique,
    tokenizer: "o200k_base (exact)",
    before_tokens: beforeCount.value,
    after_tokens: afterCount.value,
    savings_tokens: savings,
    savings_pct: round2(pct),
    injection_overhead_tokens: 0,
    // Deterministic transforms must have min == max for the post-transform
    // count (the value the case reports as after_tokens); we already
    // asserted that inside variancedCount, so reflect it here.
    variance: {
      runs: RUNS,
      min: afterCount.min,
      max: afterCount.max,
    },
    notes: args.notes,
  };
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export async function runStructural(): Promise<StructuralCase[]> {
  const cases: StructuralCase[] = [];

  cases.push(
    await runCase({
      id: "structural/json-minify",
      fixturePath: resolve(FIXTURE_ROOT, "app-config.json"),
      technique: "JSON.parse / JSON.stringify (compact)",
      transform: jsonMinify,
      notes:
        "Strict JSON in, compact JSON out via JSON.parse(text); JSON.stringify(value). Lossless by construction (no key reorder beyond V8 insertion order). Mirrors tolkin-core::format::json_minify.",
    }),
  );

  cases.push(
    await runCase({
      id: "structural/html-to-markdown",
      fixturePath: resolve(FIXTURE_ROOT, "marketing-page.html"),
      technique: "HTML -> Markdown (audit preview rules)",
      transform: htmlToMarkdown,
      notes:
        "Reproduces tolkin-core::format::html_to_markdown rules in TS so the artifact is not capped at the audit preview's 8 KB limit. Attribute data and layout semantics are dropped (near-lossless).",
    }),
  );

  cases.push(
    await runCase({
      id: "structural/paragraph-dedup",
      fixturePath: resolve(FIXTURE_ROOT, "duplicated-paragraphs.txt"),
      technique: "exact-text paragraph dedup (first-occurrence kept)",
      transform: dedupParagraphs,
      notes:
        "Split on blank lines; drop blocks whose trimmed text matches a prior block byte-for-byte. Lossless when duplicates are verbatim, which is the case the fixture exercises.",
    }),
  );

  cases.push(
    await runCase({
      id: "structural/stack-trace-dedup",
      fixturePath: resolve(FIXTURE_ROOT, "stack-trace.log"),
      technique: "stack-trace dump dedup (replace identical body with marker)",
      transform: dedupStackTrace,
      notes:
        "Split on JVM error-header lines; when two dumps share an identical post-header body, replace the second body with a one-line marker pointing at the first dump's timestamp. Lossless for the message text and structure preserved.",
    }),
  );

  return cases;
}
