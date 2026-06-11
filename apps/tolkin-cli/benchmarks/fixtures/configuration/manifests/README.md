# Vendored tools/list manifests

Real, public MCP server tool inventories, captured live and vendored verbatim
so the configuration benchmark track counts tokenized manifests instead of
catalog estimates. The benchmark runner reads these JSON files; it never
spawns a server. `capture.ts` in this directory re-captures them for
verification or refresh (see its header for usage).

Capture method, all fixtures: spawn the server over stdio and speak JSON-RPC
by hand (initialize with protocolVersion 2025-06-18, then
notifications/initialized, then tools/list), write the result object
(`{"tools": [...]}`) pretty-printed. No tools/list response was paginated
(no nextCursor). No fixture is hand-written or derived from docs; every file
is the byte-faithful JSON the server emitted, pretty-printed with 2-space
indent. Tool descriptions and schemas inside the manifests are upstream
content and are exempt from this repository's prose conventions.

Capture date for all five fixtures: 2026-06-10 (local), on macOS arm64.

## Provenance per fixture

### server-filesystem.tools.json

- Source project: https://github.com/modelcontextprotocol/servers (src/filesystem)
- Package captured: `@modelcontextprotocol/server-filesystem@2026.1.14` (npm),
  run via `bunx --yes @modelcontextprotocol/server-filesystem@2026.1.14 /tmp`
- serverInfo reported at initialize: `secure-filesystem-server` version `0.2.0`
- Tools captured: 14
- License: package.json declares MIT (author Anthropic, PBC); the repository
  LICENSE (vendored at `LICENSE-modelcontextprotocol-servers`) records the
  project's MIT to Apache-2.0 transition and includes the Apache-2.0 text.
  Both licenses permit redistribution with the license text included, which
  this directory does.

### server-memory.tools.json

- Source project: https://github.com/modelcontextprotocol/servers (src/memory)
- Package captured: `@modelcontextprotocol/server-memory@2026.1.26` (npm),
  run via `bunx --yes @modelcontextprotocol/server-memory@2026.1.26`
- serverInfo: `memory-server` version `0.6.3`
- Tools captured: 9
- License: same as server-filesystem (MIT per package.json;
  `LICENSE-modelcontextprotocol-servers` vendored).

### server-everything.tools.json

- Source project: https://github.com/modelcontextprotocol/servers (src/everything)
- Package captured: `@modelcontextprotocol/server-everything@2026.1.26` (npm),
  run via `bunx --yes @modelcontextprotocol/server-everything@2026.1.26`
- serverInfo: `mcp-servers/everything` version `2.0.0`
- Tools captured: 13
- License: same as server-filesystem (MIT per package.json;
  `LICENSE-modelcontextprotocol-servers` vendored).
- Note: this server is NOT in tolkin's curated catalog; its row demonstrates
  that manifest measurement covers servers the catalog has never seen.

### github-mcp-server.tools.json

- Source project: https://github.com/github/github-mcp-server
- Binary captured: release `v1.2.0`, asset
  `github-mcp-server_Darwin_arm64.tar.gz`, run as `github-mcp-server stdio`
  with `GITHUB_PERSONAL_ACCESS_TOKEN` set to a placeholder (the server
  registers tools without validating the token; no GitHub API call happens)
- serverInfo: `github-mcp-server` version `1.2.0`
- Toolsets: server defaults (no GITHUB_TOOLSETS set). Tools captured: 43
- License: MIT (vendored at `LICENSE-github-mcp-server`)

### github-mcp-server.slim.tools.json

- Same binary, same capture method, with `GITHUB_TOOLSETS=repos,issues`: the
  exact slim snippet tolkin's catalog recommends for this server. Tools
  captured: 27 (the default `context` toolset stays registered alongside the
  two requested toolsets; that is upstream behavior, recorded here so the
  slim row's denominator is unambiguous)
- License: MIT (vendored at `LICENSE-github-mcp-server`)

## How the benchmark counts these

The runner shells the built `tolkin` binary: `tolkin mcp <manifest> --json`.
The CLI detects the tools/list shape, canonicalizes each tool to compact
`{name, description, input_schema}` JSON (schema keys in sorted order), and
counts with o200k_base (exact). The reported cold number is the sum of the
per-tool counts: the token weight the server's tool definitions register on
a cold session, with no provider price multipliers folded in. A specific
client's wire bytes can differ by a few tokens (field order, extras like
title and annotations that clients do not forward to the model); the
canonical form is what makes the numbers reproducible byte for byte.

## Refreshing

1. `bun capture.ts` for the npm reference servers (bump the pinned versions
   in the script first).
2. Download the github-mcp-server release binary for your platform, then
   `bun capture.ts github <binary-path>`.
3. Update the provenance entries above (versions, tool counts, date).
4. Regenerate results: `bun apps/tolkin-cli/benchmarks/run.ts` and verify
   determinism with `--check-determinism`.
