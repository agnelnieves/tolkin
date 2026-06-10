#!/usr/bin/env node
// build-report.mjs
// Dependency-free Node.js script that reads a tolkin project --json report
// and emits a markdown comment suitable for a GitHub PR comment or step summary.
//
// Usage: node build-report.mjs <input.json> <output.md>
//
// Local override for testing:
//   TOLKIN_BIN=/path/to/tolkin node build-report.mjs report.json out.md

import { readFileSync, writeFileSync } from "node:fs";

const [, , inputPath, outputPath] = process.argv;
if (!inputPath || !outputPath) {
  console.error("Usage: build-report.mjs <input.json> <output.md>");
  process.exit(1);
}

const raw = readFileSync(inputPath, "utf8");
let r;
try {
  r = JSON.parse(raw);
} catch (err) {
  console.error("Failed to parse tolkin JSON output:", err.message);
  process.exit(1);
}

function commas(n) {
  return Number(n).toLocaleString("en-US");
}

function rangeStr(min, max) {
  if (min === max) return `~${commas(min)}`;
  return `~${commas(min)} to ~${commas(max)}`;
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

const profiles = r.profiles || {};
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
const totals = r.totals || {};
lines.push(
  `**Total:** ${commas(totals.context_files || 0)} agent-context files, ` +
  `${commas(totals.context_tokens || 0)} tokens in context, ` +
  `${rangeStr(totals.savings_min || 0, totals.savings_max || 0)} tokens identified reclaimable (Tier 1).`
);
lines.push("");

// MCP configs
const mcpConfigs = r.mcp_configs || [];
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
const heaviest = r.heaviest || [];
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
const findings = r.findings_by_rule || [];
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
const secrets = r.secret_files || [];
if (secrets.length > 0) {
  lines.push(
    `> **Warning:** ${secrets.length} file(s) contain likely secrets and are included in agent context. Remove them.`
  );
  lines.push("");
}

// Warnings
const warnings = r.warnings || [];
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
