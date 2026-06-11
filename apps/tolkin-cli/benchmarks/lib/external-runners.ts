// External runner adapters for the lossy comparison rows.
// repomix and cavemem are version-pinned through bun (devDependencies of
// apps/tolkin-cli/package.json) so the bench reruns are deterministic; the
// adapters here exec the installed binaries headlessly and tokenize the
// outputs through the same tolkin CLI path the other rows use. No network
// access: bun's lockfile resolves the binaries during `bun install`, and
// the runners themselves only read fixtures, write to a scratch dir, and
// shell out to a versioned local binary.

import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { REPO_ROOT } from "./cli";

// Resolved paths to the dev-dep binaries. We resolve from the cli's
// node_modules (bun's isolated layout) so the path holds in both local
// and CI installs. The :: separator ensures a hard failure if either
// binary is missing rather than silently no-oping.
const CLI_NODE_MODULES_BIN = resolve(REPO_ROOT, "apps", "tolkin-cli", "node_modules", ".bin");
export const REPOMIX_BIN = resolve(CLI_NODE_MODULES_BIN, "repomix");
export const CAVEMEM_BIN = resolve(CLI_NODE_MODULES_BIN, "cavemem");

interface ExecResult {
  code: number;
  stdout: string;
  stderr: string;
}

function execCapture(cmd: string, args: string[], cwd?: string): Promise<ExecResult> {
  return new Promise((resolveResult, rejectResult) => {
    const child = spawn(cmd, args, {
      cwd,
      env: { ...process.env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const out: Buffer[] = [];
    const err: Buffer[] = [];
    child.stdout.on("data", (b: Buffer) => out.push(b));
    child.stderr.on("data", (b: Buffer) => err.push(b));
    child.on("error", (e: Error) => rejectResult(e));
    child.on("close", (code: number | null) => {
      resolveResult({
        code: code ?? -1,
        stdout: Buffer.concat(out).toString("utf8"),
        stderr: Buffer.concat(err).toString("utf8"),
      });
    });
  });
}

/**
 * Pack a directory with repomix, with and without --compress. Returns the
 * absolute paths to the two output files (the caller is responsible for
 * tokenizing and then cleaning up the scratch dir).
 */
export interface RepomixPackResult {
  baselinePath: string;
  compressedPath: string;
  scratchDir: string;
}

export async function repomixPackBoth(corpusDir: string): Promise<RepomixPackResult> {
  const scratchDir = await mkdtemp(join(tmpdir(), "tolkin-bench-repomix-"));
  const baselinePath = join(scratchDir, "baseline.xml");
  const compressedPath = join(scratchDir, "compressed.xml");

  const baseline = await execCapture(REPOMIX_BIN, [
    corpusDir,
    "-o",
    baselinePath,
    "--quiet",
    "--style",
    "xml",
  ]);
  if (baseline.code !== 0) {
    await rm(scratchDir, { recursive: true, force: true });
    throw new Error(`repomix (baseline) exited ${baseline.code}: ${baseline.stderr.slice(0, 400)}`);
  }
  const compressed = await execCapture(REPOMIX_BIN, [
    corpusDir,
    "-o",
    compressedPath,
    "--compress",
    "--quiet",
    "--style",
    "xml",
  ]);
  if (compressed.code !== 0) {
    await rm(scratchDir, { recursive: true, force: true });
    throw new Error(
      `repomix --compress exited ${compressed.code}: ${compressed.stderr.slice(0, 400)}`,
    );
  }
  return { baselinePath, compressedPath, scratchDir };
}

/**
 * Compress a single file in place via `cavemem compress`. The CLI writes a
 * `.original` backup next to the input; the returned string is the
 * compressed text read from the input path after the rewrite. Both files
 * stay in the scratch dir so the caller can clean up with a single rm.
 */
export async function cavememCompressFile(text: string): Promise<{ compressed: string }> {
  const scratchDir = await mkdtemp(join(tmpdir(), "tolkin-bench-cavemem-"));
  const target = join(scratchDir, "fixture.txt");
  await Bun.write(target, text);
  try {
    const result = await execCapture(CAVEMEM_BIN, ["compress", target]);
    if (result.code !== 0) {
      throw new Error(`cavemem compress exited ${result.code}: ${result.stderr.slice(0, 400)}`);
    }
    const compressed = await readFile(target, "utf8");
    return { compressed };
  } finally {
    await rm(scratchDir, { recursive: true, force: true });
  }
}

/**
 * Report the resolved bin paths so the harness can sanity-check both
 * binaries land where bun's hoist resolved them before the run starts.
 */
export function externalRunnersInfo(): { repomix: string; cavemem: string } {
  return { repomix: REPOMIX_BIN, cavemem: CAVEMEM_BIN };
}
