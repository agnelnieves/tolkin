# External comparison artifacts

This directory vendors third-party transformations used by the benchmark
harness so the comparisons in `results.json` are reproducible without a
network round trip per run.

## caveman-shrink-compress.js

Vendored verbatim from JuliusBrussee/caveman, path
`src/mcp-servers/caveman-shrink/compress.js`, commit on main as of the
2026-06 harness run. Upstream license: MIT (Julius Brussee). The file is a
pure-Node prose compressor: it exports `compress`, `compressDescriptionsInPlace`,
and `withProtectedSegments`. No network access, no MCP handshake, no
runtime dependencies. The harness uses `compress(text)` to measure
caveman-shrink against Tolkin on the lossy track and
`compressDescriptionsInPlace(json, ["description"])` on MCP fixtures for
the configuration-track comparison.

To refresh:

```
curl -fsSL https://raw.githubusercontent.com/JuliusBrussee/caveman/main/src/mcp-servers/caveman-shrink/compress.js \
  -o apps/tolkin-cli/benchmarks/external/caveman-shrink-compress.js
```

The upstream LICENSE file (MIT) is reproduced at
`apps/tolkin-cli/benchmarks/external/caveman-shrink-LICENSE`.

## wilpel/caveman-compression (not vendored)

The wilpel/caveman-compression project is MIT licensed and exposes a
headless Python CLI for its NLP method, but it requires a Python virtual
environment plus the spaCy model `en_core_web_sm` (roughly 50 MB) which
this bun-only harness does not provision. The configuration-track
comparison records `status: "not-runnable-headless"` with that exact
reason so external runs can fill in the number without ambiguity.

## repomix (CLI, version-pinned via bun)

`repomix@1.14.1` is installed as a dev dependency of `apps/tolkin-cli`
(declared in its `package.json`; bun's lockfile pins the resolved
version). The benchmark runner shells the binary
`apps/tolkin-cli/node_modules/.bin/repomix`. Upstream license: MIT
(Kazuki Yamada). The LICENSE file is reproduced verbatim at
`apps/tolkin-cli/benchmarks/external/repomix-LICENSE`; source repository
https://github.com/yamadashy/repomix; npm package fetched 2026-06-10.

The published README claims `--compress` reduces token usage by about
70 percent without naming a basis. The harness runs `repomix` twice
(once with `--compress` and once without) over the self-authored corpus
at `benchmarks/fixtures/lossy/repomix-corpus/`, tokenizes both outputs
with o200k_base via the tolkin CLI, and publishes the measured saved
percent in the lossy comparison row. The upstream figure is reported in
the comparison row's `reason` field with "basis undisclosed" attached.

To refresh:

```
bun add --cwd apps/tolkin-cli --dev repomix@<version>
cp node_modules/.bun/repomix@<version>+*/node_modules/repomix/LICENSE \
   apps/tolkin-cli/benchmarks/external/repomix-LICENSE
```

## cavemem (CLI, version-pinned via bun)

`cavemem@0.2.1` is installed as a dev dependency of `apps/tolkin-cli`.
The benchmark runner shells `apps/tolkin-cli/node_modules/.bin/cavemem
compress <file>`. Upstream license: MIT (Julius Brussee). The LICENSE
file is reproduced verbatim at
`apps/tolkin-cli/benchmarks/external/cavemem-LICENSE`; source repository
https://github.com/JuliusBrussee/cavemem; npm package fetched 2026-06-10.

The upstream cavemem README's only compression figure is about 75
percent fewer prose tokens for the caveman grammar that `cavemem
compress` runs (basis undisclosed; no methodology published). A
separate about-46-percent memory-store figure appears in the
cross-reference research notes (REVIEW-FINDINGS.md) as unverified; it is
not in the upstream README and is not attributed to the upstream here.
The harness runs `cavemem compress` on the same three prose fixtures
the LLMLingua-2 cases use, tokenizes the output with o200k_base, and
publishes the measured saved percent in the lossy comparison row
alongside the upstream figure with the basis spelled out.

The `cavemem compress` subcommand is pure-JS and does not touch the
sqlite store, the embedding worker, or any network. The native
`better-sqlite3` dependency is required by other cavemem subcommands but
not loaded by `compress`; bun blocks its postinstall script and the
harness never calls a code path that needs it. `bun pm untrusted` stays
at zero.

To refresh:

```
bun add --cwd apps/tolkin-cli --dev cavemem@<version>
cp node_modules/.bun/cavemem@<version>/node_modules/cavemem/LICENSE \
   apps/tolkin-cli/benchmarks/external/cavemem-LICENSE
```

## notion-mcp-server and notion-slim

`@notionhq/notion-mcp-server` (MIT, Notion Labs, Inc.) is the upstream
Notion MCP server. Its tools/list manifest was captured live at version
2.2.1 and vendored at
`benchmarks/fixtures/configuration/manifests/notion-mcp-server.tools.json`
under its own LICENSE file
(`benchmarks/fixtures/configuration/manifests/LICENSE-notion-mcp-server`).
The configuration row measures it with the same machinery the other
manifest cases use.

`notion-slim` (MIT, mcpslim) is the slimmed fork the catalog cites as
"about 52% fewer tokens". The npm tarball ships only Windows
binaries (`mcpslim-windows-x64.exe`, `mcpslim.exe`) despite the README
claiming cross-platform support; no source code is included and no
releases are published on GitHub. This bun-on-macOS/Linux harness cannot
exercise the slim transformation headlessly, and the contract forbids
substituting a hand-built approximation. The slim row therefore appears
in the configuration comparisons table as `not-runnable-headless` with
that exact reason. The full Notion manifest is measured (the other side
of the claim's comparison) and the configuration row carries the slim
claim's denominator note.
