#!/usr/bin/env node
// run-tests.mjs
// Fixture-driven self-test for build-report.mjs. Dependency-free.
// Pins:
//   1. Delta math (sign, percent-of-base, rounding) when a baseline is passed.
//   2. The "no baseline" fallback when only HEAD is passed.
//   3. The threshold gate triggers (always-delta tokens, context-delta percent).
//   4. The threshold gate stays quiet without a baseline (push-event case).
//
// Run from anywhere:
//   node distribution/action/testdata/run-tests.mjs
//   bun distribution/action/testdata/run-tests.mjs
// Exits non-zero on the first failure.

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = resolve(HERE, "..", "build-report.mjs");
const BASE_FX = resolve(HERE, "base.json");
const HEAD_FX = resolve(HERE, "head.json");

let failed = 0;
function check(label, ok, detail) {
  if (ok) {
    console.log(`  ok   ${label}`);
  } else {
    failed += 1;
    console.log(`  FAIL ${label}`);
    if (detail) console.log(`       ${detail}`);
  }
}

function runReport({ withBase, env = {} }) {
  const dir = mkdtempSync(join(tmpdir(), "tolkin-build-report-"));
  const out = join(dir, "out.md");
  const args = [SCRIPT, HEAD_FX, out];
  if (withBase) args.push(BASE_FX);
  const res = spawnSync(process.execPath, args, {
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
  let body = "";
  try {
    body = readFileSync(out, "utf8");
  } catch {
    body = "";
  }
  rmSync(dir, { recursive: true, force: true });
  return { status: res.status, stdout: res.stdout || "", stderr: res.stderr || "", body };
}

console.log("self-test: with baseline, no gates");
{
  const { status, body } = runReport({ withBase: true });
  check("exit 0", status === 0, `got ${status}`);
  // Always loaded grew 6976 -> 11676 (+4,700, +67.4%).
  check(
    "always-loaded delta row",
    body.includes("| Always loaded | 6,976 | 11,676 | +4,700 | +67.4% |"),
    body.match(/Always loaded \|.*/)?.[0],
  );
  // Total context grew 9476 -> 14176 (+4,700, +49.6%).
  check(
    "total-context delta row",
    body.includes("| Total context | 9,476 | 14,176 | +4,700 | +49.6% |"),
    body.match(/Total context \|.*/)?.[0],
  );
  // MCP cold grew 4000 -> 5200 (+1,200, +30.0%).
  check(
    "mcp delta row",
    body.includes("| MCP cold (all configs) | 4,000 | 5,200 | +1,200 | +30.0% |"),
    body.match(/MCP cold .*/)?.[0],
  );
  // Percent column header must name the denominator.
  check("percent column names base", body.includes("Percent of base"));
  // Sticky marker still present.
  check("sticky marker", body.includes("<!-- tolkin-report -->"));
}

console.log("self-test: no baseline (push event / unresolvable base)");
{
  const { status, body } = runReport({ withBase: false });
  check("exit 0 without baseline", status === 0);
  check(
    "no-baseline note rendered",
    body.includes("No baseline available"),
  );
  // Delta rows still render with n/a placeholders so the table never disappears.
  check(
    "always row falls back to n/a",
    body.includes("| Always loaded | n/a | 11,676 | n/a | n/a |"),
    body.match(/Always loaded \|.*/)?.[0],
  );
}

console.log("self-test: always-delta threshold trips");
{
  const { status, stderr, body } = runReport({
    withBase: true,
    env: { TOLKIN_MAX_ALWAYS_DELTA_TOKENS: "1000" },
  });
  check("gate exits 3", status === 3, `got ${status}`);
  check("report still written", body.includes("Tolkin agent-context audit"));
  check(
    "gate reason on stderr",
    /TOLKIN_MAX_ALWAYS_DELTA_TOKENS=1000/.test(stderr),
    stderr,
  );
}

console.log("self-test: context-delta percent threshold trips");
{
  const { status, stderr } = runReport({
    withBase: true,
    env: { TOLKIN_MAX_CONTEXT_DELTA_PCT: "10" },
  });
  check("gate exits 3", status === 3, `got ${status}`);
  check(
    "gate reason on stderr",
    /TOLKIN_MAX_CONTEXT_DELTA_PCT=10/.test(stderr),
    stderr,
  );
}

console.log("self-test: thresholds quiet when both are below the limits");
{
  const { status } = runReport({
    withBase: true,
    env: {
      TOLKIN_MAX_ALWAYS_DELTA_TOKENS: "10000",
      TOLKIN_MAX_CONTEXT_DELTA_PCT: "100",
    },
  });
  check("exit 0 when under limits", status === 0, `got ${status}`);
}

console.log("self-test: thresholds quiet without a baseline");
{
  const { status } = runReport({
    withBase: false,
    env: {
      TOLKIN_MAX_ALWAYS_DELTA_TOKENS: "1",
      TOLKIN_MAX_CONTEXT_DELTA_PCT: "1",
    },
  });
  check("exit 0 without baseline even with strict gates", status === 0, `got ${status}`);
}

console.log("self-test: zero-base percent renders n/a, not Infinity");
{
  // Synthetic fixture pair: base with zero MCP, head with some.
  const dir = mkdtempSync(join(tmpdir(), "tolkin-zero-base-"));
  const base0 = join(dir, "base0.json");
  const head0 = join(dir, "head0.json");
  const out = join(dir, "out.md");
  writeFileSync(
    base0,
    JSON.stringify({
      profiles: { always: { tokens: 0 } },
      totals: { context_files: 0, context_tokens: 0, savings_min: 0, savings_max: 0 },
      mcp_configs: [],
    }),
  );
  writeFileSync(
    head0,
    JSON.stringify({
      profiles: { always: { tokens: 500 } },
      totals: { context_files: 1, context_tokens: 500, savings_min: 0, savings_max: 0 },
      mcp_configs: [{ path: ".mcp.json", servers: 1, cold_tokens: 800 }],
    }),
  );
  const res = spawnSync(process.execPath, [SCRIPT, head0, out, base0], { encoding: "utf8" });
  const body = readFileSync(out, "utf8");
  check("exit 0 on zero base", res.status === 0, `got ${res.status}`);
  check(
    "percent vs zero base renders n/a",
    body.includes("| Always loaded | 0 | 500 | +500 | n/a |"),
    body.match(/Always loaded \|.*/)?.[0],
  );
  rmSync(dir, { recursive: true, force: true });
}

if (failed > 0) {
  console.log(`\n${failed} check(s) failed`);
  process.exit(1);
}
console.log("\nall self-tests passed");
