// Configuration track: shell `tolkin mcp <fixture> --json` once per fixture
// and lift cold/warm/slim/pct totals into the schema. The numbers come
// directly from tolkin-core's MCP catalog, so any change to the catalog
// shifts these without any harness change.

import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { mcpAnalyze, REPO_ROOT } from "./cli";
import type { ConfigurationCase, ExternalComparison } from "./types";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE_ROOT = resolve(HERE, "..", "fixtures", "configuration");

interface CaseSpec {
  id: string;
  filename: string;
  clientShape: string;
  notes: string;
}

const CASES: CaseSpec[] = [
  {
    id: "configuration/mcp-heavy",
    filename: "mcp-heavy.json",
    clientShape: "Claude Desktop mcpServers (4 servers)",
    notes:
      "Heavy multi-server config exercising the github + slack + postgres + filesystem catalog entries. Numbers come from tolkin-core's MCP analyzer (anthropic cache math).",
  },
  {
    id: "configuration/vscode-settings",
    filename: "vscode-settings.json",
    clientShape: "VS Code settings.json mcp.servers (3 servers)",
    notes:
      "VS Code settings shape where MCP servers live under mcp.servers; same analyzer, different host config. The non-MCP keys (editor.fontSize and friends) are ignored by the analyzer.",
  },
  {
    id: "configuration/mcp-slimmed",
    filename: "mcp-slimmed.json",
    clientShape: "Claude Desktop mcpServers (1 server, already slimmed)",
    notes:
      "GITHUB_TOOLSETS env present, so the catalog scales the cold estimate and reports already-slimmed. Exercises the no-double-counting path.",
  },
];

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export async function runConfiguration(): Promise<ConfigurationCase[]> {
  const cases: ConfigurationCase[] = [];
  for (const spec of CASES) {
    const fixture = resolve(FIXTURE_ROOT, spec.filename);
    const analysis = await mcpAnalyze(fixture);
    const t = analysis.totals;
    cases.push({
      id: spec.id,
      fixture: relative(REPO_ROOT, fixture),
      client_shape: spec.clientShape,
      tokenizer: "o200k_base via tolkin mcp (anthropic provider math)",
      servers: t.servers,
      cold_tokens: t.cold,
      swap_savings_tokens: t.savings_tokens,
      slim_savings_tokens: t.slim_savings_tokens,
      pct_of_200k_window: round2(t.pct_of_window),
      notes: spec.notes,
    });
  }
  return cases;
}

/**
 * Caveman-shrink runs the same prose compressor on every `description` field
 * in the MCP fixture and reports the measured byte ratio. The harness shows
 * the measurement on the heavy fixture as the comparison artifact; the
 * configuration-track headline already comes from tolkin so the comparison
 * is descriptive, not the primary metric.
 */
export async function runConfigurationComparisons(): Promise<ExternalComparison[]> {
  const comparisons: ExternalComparison[] = [];

  // caveman-shrink compresses description text inside JSON-RPC payloads.
  // Our heavy fixture is a config file, not a tools/list response, so the
  // closest fair comparison is to round-trip a synthetic tools/list payload
  // through the same catalog and let caveman-shrink shrink the descriptions
  // it sees. Real MCP descriptions are not bundled with tolkin; we report
  // the comparison as not-comparable rather than fabricate a payload.
  comparisons.push({
    name: "caveman-shrink (JuliusBrussee/caveman, src/mcp-servers/caveman-shrink/compress.js)",
    status: "not-comparable",
    reason:
      "caveman-shrink is a runtime proxy that mutates description fields inside live tools/list responses; tolkin's MCP track measures the cold cost of registering tools from a static client config file, which has no description payload to shrink. The fixtures do not share a unit. caveman-shrink is runnable headlessly (MIT, pure Node, vendored at benchmarks/external/caveman-shrink-compress.js) and is exercised on the lossy track instead.",
  });

  comparisons.push({
    name: "wilpel/caveman-compression (NLP method)",
    status: "not-runnable-headless",
    reason:
      "MIT-licensed and exposes a Python CLI (caveman_compress_nlp.py) that runs offline, but it requires a Python virtual environment plus the spaCy en_core_web_sm model (~50 MB) which this bun-only harness does not provision. Comparable measurements can be added by running caveman_compress_nlp.py on the same MCP fixtures externally and amending this file.",
  });

  return comparisons;
}
