// RESULTS.md generation. The page agent reads results.json directly; this
// markdown is the human-facing artifact (and what stakeholders open). One
// table per track mirrors the JSON; every row names its tokenizer; the
// lossy track is fronted with the RCT caveat; the footer has the regen
// command and identifies the toolchain version.

import type { BenchResults, ExternalComparison } from "./types";

function commas(n: number): string {
  return n.toLocaleString("en-US");
}

function pct(n: number): string {
  return `${n.toFixed(2)}%`;
}

function compRow(c: ExternalComparison): string {
  if (c.status === "measured") {
    return `| ${c.name} | measured | ${commas(c.before_tokens ?? 0)} | ${commas(c.after_tokens ?? 0)} | ${pct(c.savings_pct ?? 0)} | ${c.reason} |`;
  }
  return `| ${c.name} | ${c.status} | n/a | n/a | n/a | ${c.reason} |`;
}

export function renderResults(r: BenchResults): string {
  const lines: string[] = [];
  lines.push("# Tolkin benchmark results");
  lines.push("");
  lines.push(
    "Numbers must be honest or absent. Each row names its fixture and tokenizer. Anything not measured is labeled, never silently estimated.",
  );
  lines.push("");
  lines.push("## Methodology");
  lines.push("");
  lines.push(r.methodology_md);
  lines.push("");

  // Structural.
  lines.push("## Structural track (lossless)");
  lines.push("");
  lines.push(
    "Exact before / after token counts via the tolkin CLI's o200k_base tokenizer. Injection overhead is 0 because none of these transforms requires a runtime prompt instruction.",
  );
  lines.push("");
  lines.push(
    "| Case | Fixture | Technique | Tokenizer | Before | After | Saved | Saved % | Injection | Runs |",
  );
  lines.push("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |");
  for (const c of r.tracks.structural.cases) {
    lines.push(
      `| ${c.id} | \`${c.fixture}\` | ${c.technique} | ${c.tokenizer} | ${commas(c.before_tokens)} | ${commas(c.after_tokens)} | ${commas(c.savings_tokens)} | ${pct(c.savings_pct)} | ${commas(c.injection_overhead_tokens)} | ${c.variance.runs} |`,
    );
  }
  lines.push("");
  for (const c of r.tracks.structural.cases) {
    if (c.notes) {
      lines.push(`- **${c.id}**. ${c.notes}`);
    }
  }
  lines.push("");

  // Configuration.
  lines.push("## Configuration track (lossless-configuration)");
  lines.push("");
  lines.push(
    "Each fixture is a vendored, real, public server tools/list manifest (provenance in `fixtures/configuration/manifests/README.md`), tokenized through the real CLI path (`tolkin mcp <manifest> --json`): every tool is canonicalized to compact {name, description, input_schema} JSON and counted with o200k_base (exact). Cold is the tokenized weight of the tool definitions; no provider price multipliers are folded in. Swap savings derive from the tokenized cold; slim savings are the measured difference of two tokenized manifests where a slim capture exists, otherwise 0 (never an estimate applied to a measured base).",
  );
  lines.push("");
  lines.push(
    "| Case | Fixture | Basis | Tokenizer | Tools | Cold | Swap saves | Slim saves | % of 200K window |",
  );
  lines.push("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |");
  for (const c of r.tracks.configuration.cases) {
    lines.push(
      `| ${c.id} | \`${c.fixture}\` | ${c.basis} | ${c.tokenizer} | ${commas(c.tools)} | ${commas(c.cold_tokens)} | ${commas(c.swap_savings_tokens)} | ${commas(c.slim_savings_tokens)} | ${pct(c.pct_of_200k_window)} |`,
    );
  }
  lines.push("");
  lines.push("Manifest provenance (full detail and license texts in the manifests directory):");
  lines.push("");
  for (const c of r.tracks.configuration.cases) {
    lines.push(
      `- **${c.id}**: ${c.manifest.source}; server version ${c.manifest.server_version}; captured ${c.manifest.captured}; license ${c.manifest.license}.`,
    );
  }
  lines.push("");
  if (r.tracks.configuration.comparisons.length > 0) {
    lines.push("### Configuration comparisons");
    lines.push("");
    lines.push("| Name | Status | Before | After | Saved % | Reason |");
    lines.push("| --- | --- | ---: | ---: | ---: | --- |");
    for (const c of r.tracks.configuration.comparisons) lines.push(compRow(c));
    lines.push("");
  }
  for (const c of r.tracks.configuration.cases) {
    if (c.notes) lines.push(`- **${c.id}**. ${c.notes}`);
  }
  lines.push("");

  // Lossy.
  lines.push("## Lossy track");
  lines.push("");
  lines.push(`> ${r.tracks.lossy.rct_caveat}`);
  lines.push("");
  lines.push(
    `Quality scoring: scored=${r.tracks.lossy.quality_scoring.scored}, method=${r.tracks.lossy.quality_scoring.method}. Each ratio runs the compressor once and tokenizes the output three times to enforce the deterministic-count contract.`,
  );
  lines.push("");
  lines.push(
    "| Case | Fixture | Technique | Tokenizer | Target | Before | After | Achieved | Saved % |",
  );
  lines.push("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: |");
  for (const c of r.tracks.lossy.cases) {
    lines.push(
      `| ${c.id} | \`${c.fixture}\` | ${c.technique} | ${c.tokenizer} | ${c.target_ratio.toFixed(2)} | ${commas(c.before_tokens)} | ${commas(c.after_tokens)} | ${c.achieved_ratio.toFixed(4)} | ${pct(c.savings_pct)} |`,
    );
  }
  lines.push("");
  if (r.tracks.lossy.comparisons.length > 0) {
    lines.push("### Lossy comparisons");
    lines.push("");
    lines.push("| Name | Status | Before | After | Saved % | Reason |");
    lines.push("| --- | --- | ---: | ---: | ---: | --- |");
    for (const c of r.tracks.lossy.comparisons) lines.push(compRow(c));
    lines.push("");
  }
  for (const c of r.tracks.lossy.cases) {
    if (c.notes) lines.push(`- **${c.id}**. ${c.notes}`);
  }
  lines.push("");

  // Footer.
  lines.push("---");
  lines.push("");
  lines.push(
    `Generated at: ${r.generated_at}. Tolkin ${r.tolkin_version}. Prices observed: ${r.prices_observed}. Runner: ${r.environment.runner}. OS: ${r.environment.os}.`,
  );
  lines.push("");
  lines.push(
    "To regenerate: `bun apps/tolkin-cli/benchmarks/run.ts` (from the repo root). Verify determinism with `bun apps/tolkin-cli/benchmarks/run.ts --check-determinism`.",
  );
  lines.push("");
  return lines.join("\n");
}
