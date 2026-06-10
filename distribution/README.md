# Tolkin

Privacy-first AI token analyzer for agent workflows.

Tolkin measures what your agent-context files actually cost: CLAUDE.md, AGENTS.md,
skills, MCP configs, commands, and anything else loaded into the model on your behalf.
It runs entirely on your machine. No telemetry, no uploads, no network calls except
an optional BYOK tokenizer verification you control.

Savings are reported with honest tier labels (see below). Nothing is promised that
has not been measured.

## Quickstart

```sh
# Scan local agent configs (MCP, instruction files, shell) for token waste
npx tolkin-cli scan

# Audit one file for token waste
npx tolkin-cli audit CLAUDE.md

# Repo-wide audit with load profiles
npx tolkin-cli project .

# Stats (local ledger summary)
npx tolkin-cli stats
```

All commands accept `--json` for machine-readable output. `bunx tolkin-cli` works
identically everywhere `npx` does.

## Install paths

### 1. Agent skills (universal)

Works with Claude Code, Cursor, Windsurf, Codex, and ~71 other agents that support
the `skills/<name>/SKILL.md` layout.

```sh
npx skills add <public-repo> --skill tolkin-audit
npx skills add <public-repo> --skill tolkin-slim
npx skills add <public-repo> --skill tolkin-optimize
```

Three skills are available:

| Skill | What it does |
| :--- | :--- |
| `tolkin-audit` | Repo-wide audit: run `tolkin project`, interpret findings, prioritize by severity, propose concrete edits |
| `tolkin-slim` | MCP analysis: apply slim snippets to your MCP config, verify the delta, report realized vs identified savings |
| `tolkin-optimize` | Full loop: audit, apply safe fixes (with your confirmation), re-measure, summarize with tier labels |

### 2. Claude Code plugin

Installs all three skills namespaced as `/tolkin:tolkin-audit`, `/tolkin:tolkin-slim`,
and `/tolkin:tolkin-optimize`.

```sh
/plugin marketplace add <public-repo>
/plugin install tolkin@tolkin
```

### 3. GitHub Action

Add to any repository to get a PR comment with the agent-context load profile,
heaviest files, findings, and identified savings on every pull request that touches
agent-context files.

```yaml
# .github/workflows/tolkin-audit.yml
name: Tolkin audit

on:
  pull_request:
    branches: [main]

permissions:
  contents: read
  pull-requests: write

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: <public-repo>/action@v1
        with:
          fail-on: none        # none | low | medium | high
          comment-mode: sticky # sticky | new | off
          version: "0.9.0"
```

#### Action inputs

| Input | Default | Description |
| :--- | :--- | :--- |
| `fail-on` | `none` | Exit 2 when findings at or above this severity exist. `none` always succeeds. |
| `working-directory` | `.` | Directory to audit. |
| `comment-mode` | `sticky` | `sticky`: upsert one comment per PR. `new`: always post fresh. `off`: no comment. |
| `version` | `0.9.0` | tolkin-cli version pinned for this action. |

The `permissions: pull-requests: write` block is required for `sticky` and `new`
comment modes. For `comment-mode: off` or non-PR events, `contents: read` is sufficient.

On non-pull_request events the action writes the same report to the GitHub Actions
step summary instead of a PR comment.

**Local binary override:** set `TOLKIN_BIN=/path/to/tolkin` to skip `npx` entirely.
Useful in CI environments where you build the binary from source (see `tolkin-action-dryrun.yml`
in the calling repo for an example).

## Savings tier vocabulary

Every number Tolkin surfaces belongs to exactly one tier, always labeled:

| Tier | Name | Definition |
| :--- | :--- | :--- |
| 1 | Identified | What audit/mcp/project flags as reclaimable right now. Advisory estimate. |
| 2 | Realized | Measured delta between ledger snapshots of the same project. Structural evidence. |
| 3 | Measured | Actual spend from ingested agent session logs. Ground truth. |

All savings figures are input-token bounded. Output tokens are not affected by
context slimming.

For methodology details and benchmark results, see `benchmarks/RESULTS.md` in this
repository.

## Support matrix

| Platform | Status |
| :--- | :--- |
| macOS arm64 (Apple Silicon) | Live |
| macOS x64 (Intel) | Live |
| Linux x64 | Live |
| Linux arm64 | Live |
| Windows x64 | Live |

## License

MIT. See `LICENSE`.
