// Configuration track: tokenized tools/list manifests, measured.
//
// v2 (2026-06): the track migrated from catalog estimates to vendored, real,
// public server manifests (fixtures/configuration/manifests/, provenance in
// its README.md). Each fixture is analyzed through the real CLI path
// (`tolkin mcp <manifest> --json`): the binary canonicalizes every tool to
// compact {name, description, input_schema} JSON and counts with o200k_base
// (exact). cold_tokens is the sum of the per-tool counts: the weight the
// server's tool definitions register on a cold session, with no provider
// price multipliers folded in.
//
// The three old config-shape fixtures (mcp-heavy.json, vscode-settings.json,
// mcp-slimmed.json) are retired from this track: their numbers were curated
// catalog estimates, and keeping estimate rows in a table whose claim is
// "tokenized manifests" would re-create the mixed-basis problem this
// migration closes. The fixture files stay in the repository for manual
// analyzer runs; config-shape parsing is covered by Rust tests, not by
// benchmark rows.

import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { mcpAnalyzeManifest, REPO_ROOT } from "./cli";
import type { ConfigurationCase, ExternalComparison, ManifestProvenance } from "./types";

const HERE = dirname(fileURLToPath(import.meta.url));
const MANIFEST_ROOT = resolve(HERE, "..", "fixtures", "configuration", "manifests");

const CAPTURED = "2026-06-10";
const MCP_SERVERS_LICENSE =
  "MIT per npm package.json (repo LICENSE records the MIT to Apache-2.0 transition); vendored at fixtures/configuration/manifests/LICENSE-modelcontextprotocol-servers";
const GITHUB_LICENSE =
  "MIT; vendored at fixtures/configuration/manifests/LICENSE-github-mcp-server";
const NOTION_LICENSE =
  "MIT (Copyright (c) 2025 Notion Labs, Inc.); vendored at fixtures/configuration/manifests/LICENSE-notion-mcp-server";

interface ManifestSpec {
  id: string;
  filename: string;
  clientShape: string;
  provenance: ManifestProvenance;
  /** Measured slim manifest: slim savings = this fixture's cold minus the
   * slim fixture's cold, both tokenized. Absent means the server has no
   * measured slim profile and the slim cell is 0, not an estimate. */
  slimFilename?: string;
  notes: string;
}

const CASES: ManifestSpec[] = [
  {
    id: "configuration/server-filesystem",
    filename: "server-filesystem.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source:
        "https://github.com/modelcontextprotocol/servers (src/filesystem), npm @modelcontextprotocol/server-filesystem@2026.1.14",
      license: MCP_SERVERS_LICENSE,
      captured: CAPTURED,
      server_version: "secure-filesystem-server 0.2.0 (package 2026.1.14)",
    },
    notes:
      "Reference filesystem server, captured live over stdio. Catalog entry says replace with shell builtins, so the swap savings equal the measured cold. The catalog's representative estimate for this server is 2,000 tokens; the measured manifest supersedes it.",
  },
  {
    id: "configuration/server-memory",
    filename: "server-memory.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source:
        "https://github.com/modelcontextprotocol/servers (src/memory), npm @modelcontextprotocol/server-memory@2026.1.26",
      license: MCP_SERVERS_LICENSE,
      captured: CAPTURED,
      server_version: "memory-server 0.6.3 (package 2026.1.26)",
    },
    notes:
      "Reference memory server, captured live. Catalog recommendation is keep (no CLI equivalent), so swap savings are 0 by design. The catalog's representative estimate for this server is 3,000 tokens; the measured manifest supersedes it.",
  },
  {
    id: "configuration/server-everything",
    filename: "server-everything.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source:
        "https://github.com/modelcontextprotocol/servers (src/everything), npm @modelcontextprotocol/server-everything@2026.1.26",
      license: MCP_SERVERS_LICENSE,
      captured: CAPTURED,
      server_version: "mcp-servers/everything 2.0.0 (package 2026.1.26)",
    },
    notes:
      "Reference protocol-exercise server. NOT in tolkin's curated catalog: this row exists because manifest measurement covers servers the catalog has never seen, which was the catalog's blind spot. No swap or slim recommendation, so those cells are 0.",
  },
  {
    id: "configuration/github-mcp-server",
    filename: "github-mcp-server.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source: "https://github.com/github/github-mcp-server, release v1.2.0 (Darwin arm64 asset)",
      license: GITHUB_LICENSE,
      captured: CAPTURED,
      server_version: "github-mcp-server 1.2.0, default toolsets",
    },
    slimFilename: "github-mcp-server.slim.tools.json",
    notes:
      "GitHub official server at its v1.2.0 defaults (43 tools). The catalog's representative figure is 40,000 tokens for the all-toolsets era (90 to 162 tools, externally reported 26-55K); the v1.2.0 default registration measures far smaller, which is exactly the staleness manifest measurement exists to correct. Slim savings are measured, not estimated: this cold minus the tokenized GITHUB_TOOLSETS=repos,issues manifest below.",
  },
  {
    id: "configuration/github-mcp-server-slim",
    filename: "github-mcp-server.slim.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source: "https://github.com/github/github-mcp-server, release v1.2.0 (Darwin arm64 asset)",
      license: GITHUB_LICENSE,
      captured: CAPTURED,
      server_version: "github-mcp-server 1.2.0, GITHUB_TOOLSETS=repos,issues",
    },
    notes:
      "The same binary captured with the exact slim snippet tolkin recommends (GITHUB_TOOLSETS=repos,issues; setting the env explicitly gates off the other default toolsets, including context, so this fixture is repos+issues only and the issues set registers get_label per upstream labels.go). This row IS the slim profile, so its own slim cell is 0.",
  },
  {
    // Vendored full Notion tools/list. The slim variant (notion-slim) is
    // a closed-source binary the npm tarball only ships for Windows; the
    // slim measurement therefore lives in runConfigurationComparisons as
    // not-runnable-headless. Measuring the full side closes the half of
    // the claim that IS publicly obtainable.
    id: "configuration/notion-mcp-server",
    filename: "notion-mcp-server.tools.json",
    clientShape: "tools/list manifest (single server)",
    provenance: {
      source:
        "https://github.com/makenotion/notion-mcp-server, npm @notionhq/notion-mcp-server@2.2.1",
      license: NOTION_LICENSE,
      captured: CAPTURED,
      server_version: "notion-mcp-server 2.2.1, default toolset (no env vars set)",
    },
    notes:
      "Notion's official MCP server at v2.2.1. The tools are generated from scripts/notion-openapi.json at boot; no NOTION_TOKEN is required to respond to tools/list (the manifest is derived from the OpenAPI spec, no api.notion.com call happens during the initialize/tools-list handshake). Catalog representative cold is 26,000 tokens for the 21-tool era; the measured manifest (22 tools at o200k_base) supersedes that estimate. The catalog's `notion-slim, about 52% fewer tokens` note cites a fork whose npm publication only ships Windows binaries, so the slim side of that claim ships as `not-runnable-headless` in the comparisons table below.",
  },
];

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

const WINDOW = 200_000;

export async function runConfiguration(): Promise<ConfigurationCase[]> {
  // Tokenize every manifest once up front so slim diffs reuse the same
  // numbers that land in the rows.
  const coldByFilename = new Map<string, { cold: number; tools: number; swap: number }>();
  for (const spec of CASES) {
    const manifest = resolve(MANIFEST_ROOT, spec.filename);
    const analysis = await mcpAnalyzeManifest(manifest);
    const server = analysis.servers[0];
    const detail = server.tools_detail;
    if (detail === undefined) {
      throw new Error(
        `${spec.filename}: tolkin mcp returned no tools_detail (not detected as a manifest?)`,
      );
    }
    if (!detail.basis.includes("o200k_base")) {
      throw new Error(`${spec.filename}: expected o200k_base basis, got "${detail.basis}"`);
    }
    coldByFilename.set(spec.filename, {
      cold: detail.total_tokens,
      tools: detail.tool_count,
      swap: server.savings_tokens,
    });
  }

  const cases: ConfigurationCase[] = [];
  for (const spec of CASES) {
    const measured = coldByFilename.get(spec.filename);
    if (measured === undefined) throw new Error(`missing measurement for ${spec.filename}`);
    // Slim savings: measured diff of two tokenized manifests when a slim
    // fixture exists; otherwise 0 (never a percentage estimate applied to a
    // measured base; that would be an unlabeled mixed basis).
    let slimSavings = 0;
    if (spec.slimFilename !== undefined) {
      const slim = coldByFilename.get(spec.slimFilename);
      if (slim === undefined) {
        throw new Error(`${spec.id}: slim manifest ${spec.slimFilename} is not among the cases`);
      }
      slimSavings = Math.max(0, measured.cold - slim.cold);
    }
    cases.push({
      id: spec.id,
      fixture: relative(REPO_ROOT, resolve(MANIFEST_ROOT, spec.filename)),
      client_shape: spec.clientShape,
      tokenizer: "o200k_base (exact)",
      basis: "tokenized-manifest",
      servers: 1,
      tools: measured.tools,
      cold_tokens: measured.cold,
      swap_savings_tokens: measured.swap,
      slim_savings_tokens: slimSavings,
      pct_of_200k_window: round2((measured.cold / WINDOW) * 100),
      manifest: spec.provenance,
      notes: spec.notes,
    });
  }
  return cases;
}

/**
 * caveman-shrink (vendored at benchmarks/external/caveman-shrink-compress.js)
 * is a runtime proxy that rewrites description fields inside tools/list
 * responses. Now that this track's fixtures ARE tools/list responses, the
 * comparison is measurable: shrink every description in each primary
 * manifest, re-tokenize through the same CLI path, and report the summed
 * before/after. Input-side only; the rewrite is lossy prose compression and
 * no quality claim is made.
 */
export async function runConfigurationComparisons(): Promise<ExternalComparison[]> {
  const comparisons: ExternalComparison[] = [];

  const compressJsPath = resolve(HERE, "..", "external", "caveman-shrink-compress.js");
  const mod = (await import(compressJsPath)) as {
    compressDescriptionsInPlace: (obj: unknown, fieldNames?: string[]) => void;
  };

  // The github slim fixture is the same server as the github full fixture;
  // measuring it again would double-count one binary's descriptions.
  const primary = CASES.filter((c) => c.id !== "configuration/github-mcp-server-slim");

  let totalBefore = 0;
  let totalAfter = 0;
  let notionFullCold = 0;
  const scratch = await mkdtemp(join(tmpdir(), "tolkin-bench-caveman-"));
  try {
    for (const spec of primary) {
      const manifestPath = resolve(MANIFEST_ROOT, spec.filename);
      const before = await mcpAnalyzeManifest(manifestPath);
      const beforeDetail = before.servers[0].tools_detail;
      if (beforeDetail === undefined) throw new Error(`${spec.filename}: no tools_detail`);
      // Stash the measured full Notion cold for the notion-slim comparison
      // row below: the slim cannot be measured here, but the FULL side is
      // measured and the row publishes that with the claim's denominator.
      if (spec.id === "configuration/notion-mcp-server") {
        notionFullCold = beforeDetail.total_tokens;
      }

      const parsed = JSON.parse(await readFile(manifestPath, "utf8")) as unknown;
      mod.compressDescriptionsInPlace(parsed, ["description"]);
      // Same filename in the scratch dir so the server name the CLI derives
      // from the path cannot drift between the before and after runs.
      const shrunkPath = join(scratch, spec.filename);
      await writeFile(shrunkPath, JSON.stringify(parsed, null, 2));
      const after = await mcpAnalyzeManifest(shrunkPath);
      const afterDetail = after.servers[0].tools_detail;
      if (afterDetail === undefined) throw new Error(`${spec.filename}: no tools_detail (shrunk)`);

      totalBefore += beforeDetail.total_tokens;
      totalAfter += afterDetail.total_tokens;
    }
  } finally {
    await rm(scratch, { recursive: true, force: true });
  }

  const savingsPct = totalBefore === 0 ? 0 : (1 - totalAfter / totalBefore) * 100;
  comparisons.push({
    name: "caveman-shrink (JuliusBrussee/caveman, src/mcp-servers/caveman-shrink/compress.js)",
    status: "measured",
    before_tokens: totalBefore,
    after_tokens: totalAfter,
    savings_pct: round2(savingsPct),
    reason:
      "Runnable headlessly (MIT, pure Node, vendored). Its compressDescriptionsInPlace rewrites description fields inside tools/list responses; applied to the five primary vendored manifests (the four reference servers plus the full Notion manifest; github slim excluded to avoid double-counting one server) and re-tokenized through the same tolkin CLI path (o200k_base).",
    notes:
      "Input-side effect of lossy description rewriting on real manifests; tool names and schemas untouched. No quality claim: shrunken descriptions are not evaluated for selection accuracy.",
  });

  comparisons.push({
    name: "wilpel/caveman-compression (NLP method)",
    status: "not-runnable-headless",
    reason:
      "MIT-licensed and exposes a Python CLI (caveman_compress_nlp.py) that runs offline, but it requires a Python virtual environment plus the spaCy en_core_web_sm model (~50 MB) which this bun-only harness does not provision. Comparable measurements can be added by running caveman_compress_nlp.py on the same manifest descriptions externally and amending this file.",
  });

  // notion-slim: the catalog cites "about 52% fewer tokens" against the
  // full Notion MCP. The npm package (notion-slim@2.0.0-slim.1.10) ships
  // only Windows binaries (mcpslim.exe, mcpslim-windows-x64.exe) despite
  // the README claiming macOS/Linux support; the transformation runs
  // inside a closed-source binary not present on this platform, and the
  // GitHub repository has no published release assets. The contract
  // forbids substituting a hand-built approximation of the algorithm.
  // The row therefore ships not-runnable-headless with the measured
  // full Notion cold (the upper denominator of the claim) recorded in
  // notes so a reader can derive the claimed slim count themselves and
  // re-verify when a runnable macOS/Linux build appears upstream.
  comparisons.push({
    name: "notion-slim (mcpslim/notion-slim, npm notion-slim@2.0.0-slim.1.10)",
    status: "not-runnable-headless",
    reason:
      "MIT-licensed, but the npm tarball at notion-slim@2.0.0-slim.1.10 only ships Windows binaries (bin/mcpslim.exe and bin/mcpslim-windows-x64.exe); index.js references mcpslim-darwin-arm64 and mcpslim-linux-x64 paths that are not in the tarball, the GitHub repository has no published releases, and the transformation runs inside a closed-source binary. This bun-on-macOS/Linux harness cannot exercise the slim transformation headlessly, and substituting a hand-built approximation of the recipe (recipes/notion.json grouping the 22 upstream tools into 10) is forbidden by the methodology. To verify externally: install notion-slim@<version> on Windows, capture its tools/list with the same JSON-RPC handshake as fixtures/configuration/manifests/capture.ts, vendor it next to notion-mcp-server.tools.json, add it to the configuration cases, and re-run the harness; the achieved saved percent will land next to the catalog's about-52-percent figure.",
    notes: `Full Notion manifest measured at ${notionFullCold.toLocaleString("en-US")} tokens (o200k_base, configuration/notion-mcp-server row above). Upstream claim "about 52 percent fewer tokens" is against this denominator; published basis: notion-slim package.json mcpslim.tokenReduction field and README, no methodology disclosed. Implied measured-after target if the claim holds exactly: ${Math.round(notionFullCold * 0.48).toLocaleString("en-US")} tokens.`,
  });

  return comparisons;
}
