// Shell-out helpers for the release tolkin binary. The benchmark deliberately
// invokes the same binary a user would install from npm; that way every
// number in results.json traces back to a command anyone can run.

import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { $ } from "bun";

// `$` is used in ensureBinary() only; the other helpers use Bun.spawn so we
// can pipe stdin and capture stderr without the $-template's quoting rules.

const HERE = dirname(fileURLToPath(import.meta.url));
const CLI_ROOT = resolve(HERE, "..", "..");
export const REPO_ROOT = resolve(CLI_ROOT, "..", "..");
export const TOLKIN_BIN = resolve(CLI_ROOT, "target", "release", "tolkin");

export async function ensureBinary(): Promise<void> {
  if (existsSync(TOLKIN_BIN)) return;
  // Build from the cli manifest so we land the right binary even when this
  // script is invoked from somewhere other than the cli directory.
  await $`cargo build --release --manifest-path ${CLI_ROOT}/Cargo.toml`.quiet();
  if (!existsSync(TOLKIN_BIN)) {
    throw new Error(
      `tolkin binary not found at ${TOLKIN_BIN} after cargo build; check the build output`,
    );
  }
}

export async function tolkinVersion(): Promise<string> {
  const proc = Bun.spawn([TOLKIN_BIN, "--version"], { stdout: "pipe" });
  await proc.exited;
  const out = await new Response(proc.stdout).text();
  // `tolkin 0.6.0\n` -> `0.6.0`.
  const parts = out.trim().split(/\s+/);
  return parts[1] ?? out.trim();
}

export interface CountJsonSingle {
  provider: string;
  tokens: number;
}

/**
 * Count tokens via `tolkin count --json` with the requested provider's
 * tokenizer. Streams text on stdin so big fixtures do not need a temp file.
 *
 * Uses Bun.spawn directly because the `$` template does not expose a
 * stdin-on-pipe chain method; spawn lets us pipe the fixture bytes in and
 * collect stdout synchronously without leaking a temp file.
 */
export async function countTokens(
  text: string,
  provider: "openai" | "anthropic" | "gemini" = "openai",
): Promise<number> {
  const proc = Bun.spawn([TOLKIN_BIN, "count", "--model", provider, "--json"], {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  proc.stdin.write(text);
  await proc.stdin.end();
  const code = await proc.exited;
  const out = await new Response(proc.stdout).text();
  if (code !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`tolkin count exited ${code}: ${err}`);
  }
  const parsed = JSON.parse(out) as CountJsonSingle;
  if (typeof parsed.tokens !== "number") {
    throw new Error(`tolkin count returned no tokens field: ${out}`);
  }
  return parsed.tokens;
}

export interface McpJsonTotals {
  servers: number;
  cold: number;
  warm: number;
  tool_search: number;
  savings_tokens: number;
  savings_usd: number;
  slim_savings_tokens: number;
  slim_savings_usd: number;
  pct_of_window: number;
}

export interface McpJsonResponse {
  client: string;
  provider: { name: string } | string;
  servers: Array<{
    name: string;
    tools: number | null;
    scenarios: { cold: number; warm: number; tool_search: number } | null;
    savings_tokens: number;
    cli_alternative: string | null;
    recommendation: string;
    slim: { already_slimmed: boolean; est_tokens_saved: number } | null;
    note: string;
  }>;
  totals: McpJsonTotals;
  notes: string[];
}

export async function mcpAnalyze(fixturePath: string): Promise<McpJsonResponse> {
  // Anthropic carries the cache surcharge that makes MCP cold/warm meaningful.
  const proc = Bun.spawn([TOLKIN_BIN, "mcp", "--provider", "anthropic", "--json", fixturePath], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const code = await proc.exited;
  const out = await new Response(proc.stdout).text();
  if (code !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`tolkin mcp exited ${code}: ${err}`);
  }
  return JSON.parse(out) as McpJsonResponse;
}
