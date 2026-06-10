#!/usr/bin/env bun
/**
 * Skill schema drift lint.
 *
 * Reads every distribution/skills/star/SKILL.md, finds fenced JSON blocks
 * annotated with an HTML comment above them:
 *
 *   <!-- tolkin-schema: <subcommand> [flags] -->
 *   ```json
 *   { "key": ... }
 *   ```
 *
 * For each annotated block it runs the tolkin binary against a seeded temp
 * environment, extracts all documented top-level key paths, and fails if any
 * documented key is absent from the live output.
 *
 * Also asserts that every SKILL.md metadata.version matches the version in
 * apps/tolkin-cli/Cargo.toml.
 *
 * Usage:
 *   TOLKIN_BIN=./apps/tolkin-cli/target/release/tolkin bun run \
 *     apps/tolkin-cli/scripts/check-skill-schemas.ts
 *
 * Or pass the binary path as the first argument.
 */

import { spawnSync } from "child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { dirname, join, resolve } from "path";

// ---------------------------------------------------------------------------
// Locate repo root and binary
// ---------------------------------------------------------------------------

const scriptDir = dirname(import.meta.path ?? __filename);
const repoRoot = resolve(scriptDir, "../../..");
const cargoToml = readFileSync(
  join(repoRoot, "apps/tolkin-cli/Cargo.toml"),
  "utf-8"
);

const versionMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
if (!versionMatch) {
  console.error("ERROR: could not read version from apps/tolkin-cli/Cargo.toml");
  process.exit(1);
}
const expectedVersion = versionMatch[1];

const tolkinBin: string =
  process.argv[2] ??
  process.env["TOLKIN_BIN"] ??
  join(repoRoot, "apps/tolkin-cli/target/release/tolkin");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Extract all key paths from a parsed JSON value, depth-first. Only the top
 * level is required for this lint (the spec says "top-level key path"). For
 * objects we recurse one level to handle {"tiers": {"identified": ...}} style
 * nesting. The check is: every path documented in the SKILL.md must appear in
 * the live output. We record paths as dot-separated strings.
 */
function extractKeyPaths(val: unknown, prefix = ""): string[] {
  if (val === null || typeof val !== "object" || Array.isArray(val)) {
    return prefix ? [prefix] : [];
  }
  const obj = val as Record<string, unknown>;
  const paths: string[] = [];
  for (const key of Object.keys(obj)) {
    const path = prefix ? `${prefix}.${key}` : key;
    paths.push(path);
    // One level of nesting only; deeper nesting would over-specify the schema.
    if (
      obj[key] !== null &&
      typeof obj[key] === "object" &&
      !Array.isArray(obj[key])
    ) {
      const nested = obj[key] as Record<string, unknown>;
      for (const nk of Object.keys(nested)) {
        paths.push(`${path}.${nk}`);
      }
    }
  }
  return paths;
}

/**
 * Parse the YAML-like frontmatter of a SKILL.md to read metadata.version.
 * We only need one field so a full YAML parser is unnecessary.
 */
function readSkillVersion(content: string): string | null {
  const m = content.match(/^metadata:\s*\n\s+version:\s*([^\s\n]+)/m);
  return m ? m[1] : null;
}

/**
 * Extract annotated JSON blocks from SKILL.md content.
 * Returns pairs of [command_args_string, json_text].
 */
function extractAnnotatedBlocks(
  content: string
): Array<{ command: string; jsonText: string }> {
  const results: Array<{ command: string; jsonText: string }> = [];
  // Match: <!-- tolkin-schema: <command> --> immediately followed (possibly
  // with whitespace/newlines) by a ```json ... ``` fence.
  const re =
    /<!--\s*tolkin-schema:\s*([^\s>][^>]*?)\s*-->\s*\n```(?:json)?\n([\s\S]*?)```/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    results.push({ command: m[1].trim(), jsonText: m[2] });
  }
  return results;
}

/**
 * Sanitize a schema template (which may use TypeScript-like type notation)
 * so it can be parsed as JSON. Handles:
 *   <u64>, <str>, <bool>         -> null
 *   "value1" | "value2"          -> "value1" (pick the first option)
 *   <type1> | <type2>            -> null
 *   { ... }                      -> {}
 *   [...]                        -> []
 *   } | null,                    -> }  (object-union annotation stripped)
 *   trailing commas              -> removed
 */
function sanitizeSchemaJson(raw: string): string {
  let s = raw;

  // Replace { ... } object ellipsis with empty objects
  s = s.replace(/\{\s*\.\.\.\s*\}/g, "{}");

  // Replace [...] (ellipsis arrays) with empty arrays
  s = s.replace(/\[\s*\.\.\.\s*\]/g, "[]");

  // Strip "| null" suffix after an object or array closing brace/bracket.
  // These are TypeScript-style nullable object annotations: "} | null," -> "}"
  s = s.replace(/([}\]])\s*\|\s*null/g, "$1");

  // Replace union types like: null | "value" | "other" -> pick first literal
  // Handle: "val1" | "val2" | "val3" -> keep "val1"
  s = s.replace(/"([^"]+)"(\s*\|\s*"[^"]*")+/g, '"$1"');

  // Handle: <type> | <type> -> null
  s = s.replace(/<[^>]+>(\s*\|\s*<[^>]+>)+/g, "null");

  // Handle: <type> | null -> null  or  null | <type> -> null
  s = s.replace(/<[^>]+>\s*\|\s*null/g, "null");
  s = s.replace(/null\s*\|\s*<[^>]+>/g, "null");

  // Replace remaining <type> placeholders (including compound like <str | null>)
  // These must come after | handling since they may contain |
  s = s.replace(/<[^>]+>/g, "null");

  // Replace "..." string placeholders (standalone)
  s = s.replace(/"\.\.\."/g, "null");

  // Remove trailing commas before } or ]
  s = s.replace(/,(\s*[}\]])/g, "$1");

  return s;
}

// ---------------------------------------------------------------------------
// Live binary runner
// ---------------------------------------------------------------------------

/**
 * Build a seeded temp environment with a synthetic Claude Code session log
 * and run the tolkin binary with the given subcommand args. Returns the
 * parsed JSON output, or throws on failure.
 */
function runTolkinCommand(args: string[]): unknown {
  const fakeHome = mkdtempSync(join(tmpdir(), "tolkin-schema-lint-home-"));
  const dataDir = mkdtempSync(join(tmpdir(), "tolkin-schema-lint-data-"));

  // Seed synthetic Claude Code log so --json outputs are fully populated.
  const projDir = resolve(repoRoot);
  const logDir = join(fakeHome, ".claude", "projects", "myproject");
  mkdirSync(logDir, { recursive: true });

  const now = new Date();
  const ts1 = new Date(now.getTime() - 7200_000).toISOString();
  const ts2 = new Date(now.getTime() - 3600_000).toISOString();

  const log = [
    JSON.stringify({
      message: {
        model: "claude-sonnet-4-6",
        id: "msg_lint001",
        usage: {
          input_tokens: 1200,
          output_tokens: 500,
          cache_read_input_tokens: 8000,
          cache_creation_input_tokens: 0,
          cache_creation: {
            ephemeral_5m_input_tokens: 2000,
            ephemeral_1h_input_tokens: 0,
          },
        },
      },
      requestId: "req_lint001",
      cwd: projDir,
      timestamp: ts1,
    }),
    JSON.stringify({
      message: {
        model: "claude-sonnet-4-6",
        id: "msg_lint002",
        usage: {
          input_tokens: 900,
          output_tokens: 300,
          cache_read_input_tokens: 10000,
          cache_creation_input_tokens: 500,
          cache_creation: {
            ephemeral_5m_input_tokens: 500,
            ephemeral_1h_input_tokens: 0,
          },
        },
      },
      requestId: "req_lint002",
      cwd: projDir,
      timestamp: ts2,
    }),
  ].join("\n");

  writeFileSync(join(logDir, "session001.jsonl"), log);

  // Write config with both consents enabled.
  writeFileSync(
    join(dataDir, "config.toml"),
    `v = 1\nconsent_ledger = true\nconsent_log_ingestion = true\nonboarded_at = 0\n`
  );

  // Determine the actual command parts. For commands needing a file arg
  // (like `mcp <path>`), we inject a synthetic mcp config.
  let finalArgs = [...args];
  let mcpConfig: string | null = null;

  if (args[0] === "mcp" && args.length === 2 && args[1] === "--json") {
    // `mcp --json` without a path is not valid; create a synthetic config.
    const cfgPath = join(dataDir, "mcp-test.json");
    writeFileSync(
      cfgPath,
      JSON.stringify({
        mcpServers: {
          github: {
            command: "npx",
            args: ["-y", "@modelcontextprotocol/server-github"],
          },
        },
      })
    );
    finalArgs = ["mcp", cfgPath, "--json"];
    mcpConfig = cfgPath;
  }

  // For `project --json` and `stats --json` we must first run `project .`
  // twice to seed the ledger with two snapshots (needed for realized tier).
  const needsLedger =
    args[0] === "stats" ||
    (args[0] === "project" && args.includes("--json"));

  const env: Record<string, string> = {
    ...process.env,
    HOME: fakeHome,
    TOLKIN_DATA_DIR: dataDir,
    CI: "false",
    TOLKIN_NO_LEDGER: "0",
  } as Record<string, string>;

  if (needsLedger) {
    // Seed two project snapshots.
    for (let i = 0; i < 2; i++) {
      spawnSync(tolkinBin, ["project", ".", "--json"], {
        cwd: projDir,
        env,
        encoding: "buffer",
      });
    }
  }

  const result = spawnSync(tolkinBin, finalArgs, {
    cwd: projDir,
    env,
    encoding: "utf-8",
  });

  if (result.error) {
    throw new Error(`spawn error: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(
      `tolkin ${finalArgs.join(" ")} exited ${result.status}: ${result.stderr}`
    );
  }

  const stdout = result.stdout.trim();
  // The binary may emit non-JSON lines before the JSON (e.g. init output).
  // Find the first '{' or '[' to extract the JSON portion.
  const jsonStart = stdout.search(/[{[]/);
  if (jsonStart === -1) {
    throw new Error(
      `tolkin ${finalArgs.join(" ")} produced no JSON output:\n${stdout}`
    );
  }
  const jsonText = stdout.slice(jsonStart);

  try {
    return JSON.parse(jsonText);
  } catch (e) {
    throw new Error(
      `tolkin ${finalArgs.join(" ")} output is not valid JSON:\n${jsonText.slice(0, 500)}`
    );
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const skillsDir = join(repoRoot, "distribution/skills");

let errors = 0;
let checks = 0;

const skillDirs = (() => {
  const { readdirSync } = require("fs");
  return readdirSync(skillsDir, { withFileTypes: true })
    .filter((d: { isDirectory: () => boolean }) => d.isDirectory())
    .map((d: { name: string }) => d.name);
})();

for (const skillDirName of skillDirs) {
  const skillPath = join(skillsDir, skillDirName, "SKILL.md");
  let content: string;
  try {
    content = readFileSync(skillPath, "utf-8");
  } catch {
    continue;
  }

  const skillLabel = `distribution/skills/${skillDirName}/SKILL.md`;

  // --- version check ---
  const skillVersion = readSkillVersion(content);
  if (skillVersion !== expectedVersion) {
    console.error(
      `FAIL [${skillLabel}] metadata.version is "${skillVersion}", expected "${expectedVersion}" (from apps/tolkin-cli/Cargo.toml)`
    );
    errors++;
  } else {
    console.log(`ok   [${skillLabel}] version = ${skillVersion}`);
  }
  checks++;

  // --- schema blocks ---
  const blocks = extractAnnotatedBlocks(content);
  if (blocks.length === 0) {
    console.log(`ok   [${skillLabel}] no annotated schema blocks`);
    continue;
  }

  for (const { command, jsonText } of blocks) {
    checks++;
    // Parse the documented schema (with placeholders sanitized).
    let docSchema: unknown;
    try {
      docSchema = JSON.parse(sanitizeSchemaJson(jsonText));
    } catch (e) {
      console.error(
        `FAIL [${skillLabel}] schema block for "${command}" is not parseable after sanitization: ${e}`
      );
      errors++;
      continue;
    }

    const docPaths = extractKeyPaths(docSchema);
    if (docPaths.length === 0) {
      console.log(`ok   [${skillLabel}] "${command}" schema block is empty, skipping`);
      continue;
    }

    // Run the live command.
    let liveOutput: unknown;
    const cmdArgs = command.split(/\s+/);
    try {
      liveOutput = runTolkinCommand(cmdArgs);
    } catch (e) {
      console.error(
        `FAIL [${skillLabel}] could not run "tolkin ${command}": ${e}`
      );
      errors++;
      continue;
    }

    const livePaths = extractKeyPaths(liveOutput);
    const livePathSet = new Set(livePaths);

    const missing = docPaths.filter((p) => !livePathSet.has(p));
    if (missing.length > 0) {
      for (const m of missing) {
        console.error(
          `FAIL [${skillLabel}] documented key "${m}" (command: ${command}) is absent from live output`
        );
      }
      errors++;
    } else {
      console.log(
        `ok   [${skillLabel}] "${command}" all ${docPaths.length} documented keys present in live output`
      );
    }
  }
}

console.log("");
if (errors > 0) {
  console.error(
    `skill schema check: ${errors} failure(s) across ${checks} check(s). Fix the SKILL.md files to match live binary output.`
  );
  process.exit(1);
} else {
  console.log(
    `skill schema check: all ${checks} check(s) passed.`
  );
}
