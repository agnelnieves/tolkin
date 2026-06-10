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
