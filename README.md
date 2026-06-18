# Tolkin

Privacy-first AI token analyzer for agent workflows.

Tolkin measures what your agent-context files actually cost: CLAUDE.md, AGENTS.md, skills, MCP configs, commands, and anything else loaded into the model on your behalf. It runs entirely on your machine. No telemetry, no uploads, no network calls except an optional BYOK tokenizer verification you control.

Savings are reported with honest tier labels (see below). Nothing is promised that has not been measured.

## Quickstart

```sh
# Scan local agent configs (MCP, instruction files, shell) for token waste
npx @tolkin/cli scan

# Audit one file for token waste
npx @tolkin/cli audit CLAUDE.md

# Repo-wide audit with load profiles
npx @tolkin/cli project .

# Stats (local ledger summary)
npx @tolkin/cli stats
```

All commands accept `--json` for machine-readable output. `bunx @tolkin/cli` works identically everywhere `npx` does.

## Install

### 1. Agent skills (universal)

Works with Claude Code, Cursor, Windsurf, Codex, and ~71 other agents that support the `skills/<name>/SKILL.md` layout.

```sh
npx skills add agnelnieves/tolkin --skill tolkin-audit
npx skills add agnelnieves/tolkin --skill tolkin-slim
npx skills add agnelnieves/tolkin --skill tolkin-optimize
npx skills add agnelnieves/tolkin --skill tolkin-cache
```

Four skills are available:

| Skill | What it does |
| :--- | :--- |
| `tolkin-audit` | Repo-wide audit: run `tolkin project`, interpret findings, prioritize by severity, propose concrete edits |
| `tolkin-slim` | MCP analysis: apply slim snippets to your MCP config, verify the delta, report realized vs identified savings |
| `tolkin-optimize` | Full loop: audit, apply safe fixes (with your confirmation), re-measure, summarize with tier labels |
| `tolkin-cache` | Prompt-cache health: read the measured report, apply prefix-stability fixes (with your confirmation), re-measure |

### 2. Claude Code plugin

Installs all four skills namespaced as `/tolkin:tolkin-audit`, `/tolkin:tolkin-slim`, `/tolkin:tolkin-optimize`, and `/tolkin:tolkin-cache`.

```sh
/plugin marketplace add agnelnieves/tolkin
/plugin install tolkin@tolkin
```

### 3. Homebrew (macOS and Linux)

Tap the repository once, then install:

```sh
brew tap agnelnieves/tolkin
brew install tolkin
```

Or install in a single command without a prior tap:

```sh
brew install agnelnieves/tolkin/tolkin
```

Recent Homebrew versions gate third-party taps behind a one-time trust decision: interactive shells prompt for it during install; scripts and CI must run `brew trust agnelnieves/tolkin` once before `brew install`.

The tap is at `agnelnieves/homebrew-tolkin`. Homebrew covers macOS arm64 (Apple Silicon), macOS x64 (Intel), Linux x64, and Linux arm64. Windows users should use `npx @tolkin/cli` instead. Bottles and source builds from homebrew-core are out of scope until the project reaches the OSS extraction milestone; this tap provides pre-built binary installs only.

#### Upgrading

```sh
brew update && brew upgrade tolkin
```

Always run the two commands together. Third-party taps only refresh during `brew update`, so `brew upgrade tolkin` on its own can answer "already installed" even when a newer release exists (Homebrew's auto-update may sit on tap metadata for up to a day). If tolkin came from npm instead, the command is `npm update -g @tolkin/cli`. Not sure which one applies? Run `tolkin update`: it checks the registry once, detects the install channel, and prints the exact command to copy.

### 4. npm (global or local)

```sh
npm install -g @tolkin/cli
```

Or in a project:

```sh
npm install --save-dev @tolkin/cli
npx @tolkin/cli <command>
```

### 5. GitHub Action

Add to any repository to get a PR comment with the agent-context load profile, heaviest files, findings, and identified savings on every pull request that touches agent-context files.

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
      - uses: agnelnieves/tolkin/action@v1
        with:
          fail-on: none        # none | low | medium | high
          comment-mode: sticky # sticky | new | off
          version: "0.15.1"
```

#### Action inputs

| Input | Default | Description |
| :--- | :--- | :--- |
| `fail-on` | `none` | Exit 2 when findings at or above this severity exist. `none` always succeeds. |
| `working-directory` | `.` | Directory to audit. |
| `comment-mode` | `sticky` | `sticky`: upsert one comment per PR. `new`: always post fresh. `off`: no comment. |
| `version` | `0.15.1` | tolkin-cli version pinned for this action. |

The `permissions: pull-requests: write` block is required for `sticky` and `new` comment modes. For `comment-mode: off` or non-PR events, `contents: read` is sufficient.

On non-pull_request events the action writes the same report to the GitHub Actions step summary instead of a PR comment.

**Local binary override:** set `TOLKIN_BIN=/path/to/tolkin` to skip `npx` entirely. Useful in CI environments where you build the binary from source (see `tolkin-action-dryrun.yml` in the calling repo for an example).

## Savings tier vocabulary

Every number Tolkin surfaces belongs to exactly one tier, always labeled:

| Tier | Name | Definition |
| :--- | :--- | :--- |
| 1 | Identified | What audit/mcp/project flags as reclaimable right now. Advisory estimate. |
| 2 | Realized | Measured delta between ledger snapshots of the same project. Structural evidence. |
| 3 | Measured | Actual spend from ingested agent session logs. Ground truth. |

All savings figures are input-token bounded. Output tokens are not affected by context slimming.

For methodology details and benchmark results, see `benchmarks/RESULTS.md` in this repository.

## Support matrix

| Platform | npx / bunx | Homebrew | npm |
| :--- | :--- | :--- | :--- |
| macOS arm64 (Apple Silicon) | Live | Live (tap) | Live |
| macOS x64 (Intel) | Live | Live (tap) | Live |
| Linux x64 | Live | Live (tap) | Live |
| Linux arm64 | Live | Live (tap) | Live |
| Windows x64 | Live | Not supported (use npx) | Live |

## Privacy

See [PRIVACY.md](PRIVACY.md) for the full privacy posture: zero network egress from the analyzer, what the local ledger stores, log ingestion scope, and the env variables that disable persistence.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build, test, and contribute.

## Security

See [SECURITY.md](SECURITY.md) for vulnerability disclosure and security policy.

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). This project is committed to fostering an inclusive and respectful community.

## License

MIT. See [LICENSE](LICENSE).
