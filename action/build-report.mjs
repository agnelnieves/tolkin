#!/usr/bin/env node
// build-report.mjs
// Dependency-free Node.js script that reads a tolkin project --json report
// and emits a markdown comment suitable for a GitHub PR comment or step
// summary. Optionally takes a baseline JSON (produced from the PR's merge
// base) and renders delta columns (base, head, delta, percent vs base) for
// always-loaded tokens, total context tokens, and MCP cold tokens.
//
// Usage:
//   node build-report.mjs <head.json> <output.md>
//   node build-report.mjs <head.json> <output.md> <base.json>
//
// Threshold gating (default off): set the following env vars to enable.
// When either trips, the script writes the report and exits non-zero with
// a one-line reason on stderr.
//   TOLKIN_MAX_ALWAYS_DELTA_TOKENS  fail when always-loaded grew by more
//                                    than N tokens vs base
//   TOLKIN_MAX_CONTEXT_DELTA_PCT    fail when total context tokens grew by
//                                    more than X percent vs base
//
// When no baseline is supplied (push events, unresolvable base, first
// commit), thresholds are not evaluated and a quiet "no baseline" note is
// appended to the report.
//
// Local override for testing:
//   TOLKIN_BIN=/path/to/tolkin node build-report.mjs report.json out.md

import { readFileSync, writeFileSync } from "node:fs";

const [, , inputPath, outputPath, basePath] = process.argv;
if (!inputPath || !outputPath) {
  console.error("Usage: build-report.mjs <head.json> <output.md> [base.json]");
  process.exit(1);
}

function parseJsonFile(path, label) {
  const raw = readFileSync(path, "utf8");
  try {
    return JSON.parse(raw);
  } catch (err) {
    console.error(`Failed to parse ${label} tolkin JSON output:`, err.message);
    process.exit(1);
  }
}

const head = parseJsonFile(inputPath, "head");
const base = basePath ? parseJsonFile(basePath, "base") : null;

function commas(n) {
  return Number(n).toLocaleString("en-US");
}

function rangeStr(min, max) {
  if (min === max) return `~${commas(min)}`;
  return `~${commas(min)} to ~${commas(max)}`;
}

// Signed integer with explicit sign, e.g. "+4,700" or "-128".
function signed(n) {
  const v = Number(n);
  if (v > 0) return `+${commas(v)}`;
  if (v < 0) return `${commas(v)}`;
  return "0";
}

// Signed percent of the base, e.g. "+33.5%" or "0.0%". Returns "n/a" when
// the base value is zero (percent is undefined or infinite there).
function signedPct(delta, baseVal) {
  const b = Number(baseVal);
  if (b === 0) return "n/a";
  const pct = (Number(delta) / b) * 100;
  const rounded = Math.round(pct * 10) / 10;
  if (rounded > 0) return `+${rounded.toFixed(1)}%`;
  if (rounded < 0) return `${rounded.toFixed(1)}%`;
  return "0.0%";
}

// MCP configs are keyed by path. Sum cold_tokens across configs so the
// delta row is well-defined even when configs appear or disappear.
function totalMcpColdTokens(report) {
  const cfgs = report?.mcp_configs || [];
  let sum = 0;
  for (const c of cfgs) sum += Number(c.cold_tokens || 0);
  return sum;
}

const lines = [];

lines.push("<!-- tolkin-report -->");
lines.push("## Tolkin agent-context audit");
lines.push("");

// Load profile table
lines.push("### Context load profile");
lines.push("");
lines.push("| Profile | Tokens | Notes |");
lines.push("| :--- | ---: | :--- |");

const profiles = head.profiles || {};
const always = profiles.always || { tokens: 0, files: [] };
const onInvoke = profiles.on_invocation || { tokens: 0, files: [] };
const onDemand = profiles.on_demand || { tokens: 0, files: [] };
const docs = profiles.docs || { tokens: 0, files: [] };

lines.push(`| Always loaded | ${commas(always.tokens)} | sent on every session start |`);
lines.push(`| On invocation | ${commas(onInvoke.tokens)} | loaded when a skill or command is invoked |`);
lines.push(`| On demand | ${commas(onDemand.tokens)} | pulled in mid-session |`);
lines.push(`| Docs | ${commas(docs.tokens)} | root markdown files |`);
lines.push("");

// Totals line
const totals = head.totals || {};
lines.push(
  `**Total:** ${commas(totals.context_files || 0)} agent-context files, ` +
  `${commas(totals.context_tokens || 0)} tokens in context, ` +
  `${rangeStr(totals.savings_min || 0, totals.savings_max || 0)} tokens identified reclaimable (Tier 1).`
);
lines.push("");

// Delta section vs base.
// Three rows: always-loaded, total context, MCP cold (across all configs).
// Percent column is always "of base"; that denominator is named in the
// column header so no number is rendered without its denominator.
const headAlways = Number(always.tokens || 0);
const headContext = Number(totals.context_tokens || 0);
const headMcp = totalMcpColdTokens(head);

let baseAlways = 0;
let baseContext = 0;
let baseMcp = 0;
if (base) {
  baseAlways = Number(base?.profiles?.always?.tokens || 0);
  baseContext = Number(base?.totals?.context_tokens || 0);
  baseMcp = totalMcpColdTokens(base);
}

const dAlways = headAlways - baseAlways;
const dContext = headContext - baseContext;
const dMcp = headMcp - baseMcp;

lines.push("### Change vs base");
lines.push("");
lines.push("| Metric | Base | Head | Delta | Percent of base |");
lines.push("| :--- | ---: | ---: | ---: | ---: |");

if (base) {
  lines.push(
    `| Always loaded | ${commas(baseAlways)} | ${commas(headAlways)} | ${signed(dAlways)} | ${signedPct(dAlways, baseAlways)} |`,
  );
  lines.push(
    `| Total context | ${commas(baseContext)} | ${commas(headContext)} | ${signed(dContext)} | ${signedPct(dContext, baseContext)} |`,
  );
  // Only render the MCP row when either side actually has MCP configs;
  // a "0 to 0, 0%" row is just noise.
  if (headMcp > 0 || baseMcp > 0) {
    lines.push(
      `| MCP cold (all configs) | ${commas(baseMcp)} | ${commas(headMcp)} | ${signed(dMcp)} | ${signedPct(dMcp, baseMcp)} |`,
    );
  }
} else {
  lines.push(
    `| Always loaded | n/a | ${commas(headAlways)} | n/a | n/a |`,
  );
  lines.push(
    `| Total context | n/a | ${commas(headContext)} | n/a | n/a |`,
  );
  if (headMcp > 0) {
    lines.push(
      `| MCP cold (all configs) | n/a | ${commas(headMcp)} | n/a | n/a |`,
    );
  }
  lines.push("");
  lines.push("_No baseline available; comparison columns are blank. Absolute totals stay above._");
}
lines.push("");

// MCP configs
const mcpConfigs = head.mcp_configs || [];
if (mcpConfigs.length > 0) {
  lines.push("### MCP configs");
  lines.push("");
  lines.push("| Config | Servers | Cold tokens |");
  lines.push("| :--- | ---: | ---: |");
  for (const m of mcpConfigs) {
    lines.push(`| \`${m.path}\` | ${m.servers} | ${commas(m.cold_tokens)} |`);
  }
  lines.push("");
}

// Heaviest files
const heaviest = head.heaviest || [];
if (heaviest.length > 0) {
  lines.push("### Heaviest agent-context files");
  lines.push("");
  lines.push("| File | Tokens | Findings | Identified reclaimable (Tier 1) |");
  lines.push("| :--- | ---: | ---: | ---: |");
  for (const h of heaviest) {
    const rec =
      h.savings_max > 0 ? rangeStr(h.savings_min, h.savings_max) : "-";
    lines.push(
      `| \`${h.path}\` | ${commas(h.tokens)} | ${h.findings} | ${rec} |`
    );
  }
  lines.push("");
}

// Findings by rule
const findings = head.findings_by_rule || [];
if (findings.length > 0) {
  lines.push("### Findings by rule");
  lines.push("");
  lines.push("| Rule | Count | Identified reclaimable (Tier 1) |");
  lines.push("| :--- | ---: | ---: |");
  for (const f of findings) {
    lines.push(
      `| ${f.rule} | ${f.count} | ${rangeStr(f.savings_min, f.savings_max)} tokens |`
    );
  }
  lines.push("");
} else {
  lines.push("No findings. Agent context is lean.");
  lines.push("");
}

// Secrets
const secrets = head.secret_files || [];
if (secrets.length > 0) {
  lines.push(
    `> **Warning:** ${secrets.length} file(s) contain likely secrets and are included in agent context. Remove them.`
  );
  lines.push("");
}

// Warnings
const warnings = head.warnings || [];
for (const w of warnings) {
  lines.push(`> Note: ${w}`);
}
if (warnings.length > 0) lines.push("");

// Honesty footer
lines.push(
  "_Identified savings (Tier 1) are input-token estimates based on current file content. " +
  "They become realized (Tier 2) only after edits are applied and the project is re-analyzed. " +
  "All numbers are input-token bounded; output tokens are not affected. " +
  "Nothing leaves your machine._"
);

writeFileSync(outputPath, lines.join("\n") + "\n");
console.log(`Report written to ${outputPath} (${lines.length} lines)`);

// Threshold gating, after the report is written so the comment always
// shows up even when the gate trips.
const maxAlwaysRaw = process.env.TOLKIN_MAX_ALWAYS_DELTA_TOKENS;
const maxCtxPctRaw = process.env.TOLKIN_MAX_CONTEXT_DELTA_PCT;

const reasons = [];

if (base && maxAlwaysRaw && maxAlwaysRaw.trim() !== "") {
  const limit = Number(maxAlwaysRaw);
  if (Number.isFinite(limit) && dAlways > limit) {
    reasons.push(
      `always-loaded context grew by ${signed(dAlways)} tokens vs base, ` +
      `exceeding TOLKIN_MAX_ALWAYS_DELTA_TOKENS=${limit}`,
    );
  }
}

if (base && maxCtxPctRaw && maxCtxPctRaw.trim() !== "") {
  const limit = Number(maxCtxPctRaw);
  if (Number.isFinite(limit) && baseContext > 0) {
    const pct = (dContext / baseContext) * 100;
    if (pct > limit) {
      reasons.push(
        `total context grew by ${pct.toFixed(1)}% vs base, ` +
        `exceeding TOLKIN_MAX_CONTEXT_DELTA_PCT=${limit}`,
      );
    }
  }
}

if (reasons.length > 0) {
  for (const r of reasons) console.error(`tolkin delta gate: ${r}`);
  process.exit(3);
}
