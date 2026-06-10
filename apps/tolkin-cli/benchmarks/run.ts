// Tolkin benchmark harness, entrypoint.
//
// Run:        bun apps/tolkin-cli/benchmarks/run.ts
// Determinism: bun apps/tolkin-cli/benchmarks/run.ts --check-determinism
//
// The runner ensures the tolkin release binary exists, then drives three
// tracks against the fixtures under benchmarks/fixtures/. Outputs land at
// benchmarks/results.json (the canonical artifact), benchmarks/RESULTS.md
// (the human-facing version), and apps/tolkin-web/src/data/bench-results.
// json (the synced copy the /bench page imports).
//
// Determinism contract: two runs against the same tree must produce
// results.json files that differ only at `generated_at`. The check flag
// runs the whole pipeline twice and diffs the outputs to enforce this.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ensureBinary, REPO_ROOT, tolkinVersion } from "./lib/cli";
import { runConfiguration, runConfigurationComparisons } from "./lib/configuration";
import { runLossy, runLossyComparisons } from "./lib/lossy";
import { renderResults } from "./lib/render";
import { runStructural } from "./lib/structural";
import type { BenchResults } from "./lib/types";

const HERE = dirname(fileURLToPath(import.meta.url));
const BENCH_ROOT = HERE;
const RESULTS_JSON = resolve(BENCH_ROOT, "results.json");
const RESULTS_MD = resolve(BENCH_ROOT, "RESULTS.md");
const METHODOLOGY_MD = resolve(BENCH_ROOT, "methodology.md");
const SYNCED_JSON = resolve(REPO_ROOT, "apps", "tolkin-web", "src", "data", "bench-results.json");

const RUNS = 3;

// PRICES_OBSERVED is tracked by tolkin-core::pricing (see crates/core/src/
// pricing.rs). Hardcoding here mirrors the marker that ships in the CLI's
// `tolkin cost` output for 0.6.0; if pricing.rs ever changes this string,
// bump it here too. The string is part of the determinism contract.
const PRICES_OBSERVED = "2026-06";

function loadMethodology(): string {
  if (existsSync(METHODOLOGY_MD)) {
    return readFileSync(METHODOLOGY_MD, "utf8").trim();
  }
  return "methodology pending";
}

function envInfo(): { runner: string; os: string; tokenizers: Record<string, string> } {
  // Stable strings, no timestamps and no per-machine details that would
  // cause harmless drift between local and CI runs.
  return {
    runner: "bun + tolkin CLI",
    os: process.platform,
    tokenizers: {
      openai: "o200k_base (exact, via tolkin count --json --model openai)",
      anthropic: "cl100k_base proxy, labeled estimate (+/-10%)",
      gemini: "Gemma SPM (exact)",
    },
  };
}

async function buildResults(generatedAt: string): Promise<BenchResults> {
  await ensureBinary();
  const tokVersion = await tolkinVersion();
  const [structural, configuration, configComps, lossy, lossyComps] = await Promise.all([
    runStructural(),
    runConfiguration(),
    runConfigurationComparisons(),
    runLossy(),
    runLossyComparisons(),
  ]);
  const r: BenchResults = {
    v: 1,
    generated_at: generatedAt,
    tolkin_version: tokVersion,
    prices_observed: PRICES_OBSERVED,
    runs: RUNS,
    environment: envInfo(),
    methodology_md: loadMethodology(),
    tracks: {
      structural: { fidelity: "lossless", cases: structural },
      configuration: {
        fidelity: "lossless-configuration",
        cases: configuration,
        comparisons: configComps,
      },
      lossy: {
        fidelity: "lossy",
        rct_caveat:
          "Aggressive compression can increase total cost because outputs grow; every number here is input-token bounded.",
        quality_scoring: { scored: false, method: "BYOK extraction-QA harness, off by default" },
        cases: lossy,
        comparisons: lossyComps,
      },
    },
  };
  return r;
}

function ensureDataDir(): void {
  mkdirSync(dirname(SYNCED_JSON), { recursive: true });
}

function writeArtifacts(r: BenchResults): void {
  // Pretty-print with 2-space indent and a trailing newline so the file is
  // diff-friendly in git and reproducible across runs.
  const json = `${JSON.stringify(r, null, 2)}\n`;
  writeFileSync(RESULTS_JSON, json);
  ensureDataDir();
  writeFileSync(SYNCED_JSON, json);
  writeFileSync(RESULTS_MD, renderResults(r));
}

interface DiffSummary {
  diffOnlyGeneratedAt: boolean;
  unexpectedKeys: string[];
}

function diffResults(a: BenchResults, b: BenchResults): DiffSummary {
  // Compare by-JSON-string, scrubbing generated_at on both sides. If the
  // scrubbed strings are equal, the determinism contract holds.
  const scrub = (r: BenchResults): string => {
    const copy = { ...r, generated_at: "<scrubbed>" };
    return JSON.stringify(copy, null, 2);
  };
  const sa = scrub(a);
  const sb = scrub(b);
  if (sa === sb)
    return { diffOnlyGeneratedAt: a.generated_at !== b.generated_at, unexpectedKeys: [] };
  // Find a few keys that differ to surface a useful error.
  const flat = (obj: unknown, prefix = ""): Map<string, string> => {
    const out = new Map<string, string>();
    const walk = (v: unknown, p: string) => {
      if (v === null || typeof v !== "object") {
        out.set(p, JSON.stringify(v));
        return;
      }
      if (Array.isArray(v)) {
        v.forEach((item, i) => walk(item, `${p}[${i}]`));
        return;
      }
      for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
        walk(val, p ? `${p}.${k}` : k);
      }
    };
    walk(obj, prefix);
    return out;
  };
  const fa = flat({ ...a, generated_at: "<scrubbed>" });
  const fb = flat({ ...b, generated_at: "<scrubbed>" });
  const keys = new Set<string>([...fa.keys(), ...fb.keys()]);
  const drift: string[] = [];
  for (const k of keys) {
    if (fa.get(k) !== fb.get(k)) drift.push(k);
  }
  return { diffOnlyGeneratedAt: false, unexpectedKeys: drift.slice(0, 10) };
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const check = args.includes("--check-determinism");

  if (check) {
    const r1 = await buildResults(new Date().toISOString());
    const r2 = await buildResults(new Date().toISOString());
    const diff = diffResults(r1, r2);
    if (!diff.diffOnlyGeneratedAt) {
      const why =
        diff.unexpectedKeys.length > 0
          ? `unexpected drift at: ${diff.unexpectedKeys.join(", ")}`
          : "generated_at did not change between runs (clock granularity issue?)";
      throw new Error(`determinism check failed: ${why}`);
    }
    // Side-effect: write the second run as the canonical artifact.
    writeArtifacts(r2);
    process.stdout.write("determinism contract holds: results differ only at generated_at\n");
    return;
  }

  const r = await buildResults(new Date().toISOString());
  writeArtifacts(r);
  process.stdout.write(`wrote ${RESULTS_JSON}, ${RESULTS_MD}, and ${SYNCED_JSON}\n`);
}

main().catch((e: unknown) => {
  const msg = e instanceof Error ? e.message : String(e);
  process.stderr.write(`bench harness failed: ${msg}\n`);
  if (e instanceof Error && e.stack) process.stderr.write(`${e.stack}\n`);
  process.exit(1);
});
