#!/usr/bin/env bun
// Capture tools/list manifests from real MCP servers for the configuration
// benchmark track. NOT invoked by the benchmark runner (run.ts reads the
// vendored JSON files); this script exists so anyone can re-capture the
// fixtures and diff them against what is vendored.
//
// Method: spawn the server over stdio, speak JSON-RPC by hand
// (initialize -> notifications/initialized -> tools/list), write the result
// object ({"tools": [...]}) pretty-printed. No MCP SDK dependency.
//
// Usage:
//   bun capture.ts                       captures the three npm reference
//                                        servers at the pinned versions below
//   bun capture.ts github <binary-path>  captures github-mcp-server full and
//                                        slim (GITHUB_TOOLSETS=repos,issues)
//                                        from a downloaded release binary
//
// github-mcp-server ships as a Go binary; download the release asset for
// your platform first, for example:
//   curl -sL -o ghmcp.tar.gz https://github.com/github/github-mcp-server/releases/download/v1.2.0/github-mcp-server_Darwin_arm64.tar.gz
//   tar xzf ghmcp.tar.gz
// The server lists tools without a valid token; GITHUB_PERSONAL_ACCESS_TOKEN
// only needs to be set, not real, and no GitHub API call is made.

import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

// Pinned versions: bump deliberately and re-record provenance in README.md.
const NPM_SERVERS: Array<{ out: string; pkg: string; args: string[] }> = [
  {
    out: "server-filesystem.tools.json",
    pkg: "@modelcontextprotocol/server-filesystem@2026.1.14",
    args: ["/tmp"],
  },
  {
    out: "server-memory.tools.json",
    pkg: "@modelcontextprotocol/server-memory@2026.1.26",
    args: [],
  },
  {
    out: "server-everything.tools.json",
    pkg: "@modelcontextprotocol/server-everything@2026.1.26",
    args: [],
  },
  {
    out: "notion-mcp-server.tools.json",
    pkg: "@notionhq/notion-mcp-server@2.2.1",
    args: [],
  },
];

interface JsonRpcMessage {
  id?: number;
  result?: {
    serverInfo?: { name?: string; version?: string };
    tools?: unknown[];
  };
}

async function captureToolsList(
  cmd: string[],
  env: Record<string, string>,
): Promise<{ result: unknown; serverInfo: unknown }> {
  const proc = Bun.spawn(cmd, {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, ...env },
  });
  const enc = new TextEncoder();
  const send = (obj: unknown) => proc.stdin.write(enc.encode(`${JSON.stringify(obj)}\n`));

  send({
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "tolkin-bench-capture", version: "0.0.0" },
    },
  });

  const reader = proc.stdout.getReader();
  const dec = new TextDecoder();
  let buf = "";
  let serverInfo: unknown = null;
  const deadline = Date.now() + 120_000;

  while (Date.now() < deadline) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let nl = buf.indexOf("\n");
    while (nl !== -1) {
      const line = buf.slice(0, nl).trim();
      buf = buf.slice(nl + 1);
      nl = buf.indexOf("\n");
      if (line === "") continue;
      let msg: JsonRpcMessage;
      try {
        msg = JSON.parse(line) as JsonRpcMessage;
      } catch {
        continue;
      }
      if (msg.id === 1 && serverInfo === null) {
        serverInfo = msg.result?.serverInfo ?? {};
        send({ jsonrpc: "2.0", method: "notifications/initialized" });
        send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
      } else if (msg.id === 2) {
        proc.kill();
        if (msg.result?.tools === undefined) {
          throw new Error(`tools/list returned no tools: ${line.slice(0, 200)}`);
        }
        if ("nextCursor" in (msg.result as Record<string, unknown>)) {
          throw new Error(
            "tools/list is paginated (nextCursor present); extend this script to page before vendoring",
          );
        }
        return { result: msg.result, serverInfo };
      }
    }
  }
  proc.kill();
  throw new Error(`timed out waiting for tools/list from: ${cmd.join(" ")}`);
}

async function writeManifest(out: string, result: unknown, serverInfo: unknown): Promise<void> {
  const path = resolve(HERE, out);
  await Bun.write(path, `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`wrote ${out} (serverInfo: ${JSON.stringify(serverInfo)})\n`);
}

const mode = process.argv[2];

if (mode === "github") {
  const binary = process.argv[3];
  if (!binary) {
    process.stderr.write("usage: bun capture.ts github <path-to-github-mcp-server-binary>\n");
    process.exit(1);
  }
  const placeholderEnv = { GITHUB_PERSONAL_ACCESS_TOKEN: "ghp_placeholder_not_a_real_token" };
  const full = await captureToolsList([binary, "stdio"], placeholderEnv);
  await writeManifest("github-mcp-server.tools.json", full.result, full.serverInfo);
  const slim = await captureToolsList([binary, "stdio"], {
    ...placeholderEnv,
    GITHUB_TOOLSETS: "repos,issues",
  });
  await writeManifest("github-mcp-server.slim.tools.json", slim.result, slim.serverInfo);
} else {
  for (const server of NPM_SERVERS) {
    const { result, serverInfo } = await captureToolsList(
      ["bunx", "--yes", server.pkg, ...server.args],
      {},
    );
    await writeManifest(server.out, result, serverInfo);
  }
}
