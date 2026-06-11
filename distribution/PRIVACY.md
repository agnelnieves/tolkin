# Tolkin Privacy Posture

This document states what data Tolkin collects, stores, and transmits.
Every claim is cross-referenced to the source file that implements it.

## Zero network egress from the analyzer

The CLI runs entirely on the local machine. No token counts, file contents,
project paths, or user identifiers are transmitted to any remote server.

The only sanctioned outbound request is the opt-in BYOK token verification:
`tolkin count --verify <file>` sends the file's text to
`https://api.anthropic.com/v1/messages/count_tokens` using the API key the
user supplies on the command line. This request never runs without an explicit
`--verify` flag and an explicit key argument. The rest of the CLI, including
all audit, scan, stats, cache, and project commands, makes no network calls.

Source: `apps/tolkin-cli/src/verify.rs`. The file contains exactly one
outbound endpoint (`const ENDPOINT`) and is only reachable from
`apps/tolkin-cli/src/commands/count.rs` behind the `--verify` flag.

The benchmarks tooling (`benchmarks/`) fetches a model file once as a
development-time artifact. This fetch is not part of the shipped CLI binary
and does not run during normal operation.

## The local ledger

### What it stores

The ledger is an append-only JSONL file (`ledger.jsonl`). Each record carries:
headline token counts (always-loaded tokens, reclaimable range, MCP cold
tokens), the command name, the project directory path (as an absolute path
string), a timestamp (unix epoch seconds), the tolkin version, and the
observed pricing label. The ledger never stores file contents, secret values,
raw output from analyzed files, or message content.

Source: `apps/tolkin-cli/src/ledger.rs`, function `append_in`. The record
shape is the `json!({...})` literal in that function; all fields are
enumerably scalar (u64 counts, timestamps, version strings).

### Where it lives

Default location: the platform data directory as resolved by the `directories`
crate, under the `tolkin` project name. On macOS this is
`~/Library/Application Support/tolkin/`; on Linux `~/.local/share/tolkin/`;
on Windows `%APPDATA%\tolkin\`. The `TOLKIN_DATA_DIR` environment variable
overrides the location entirely; setting it to a temp directory (as in
`TOLKIN_DATA_DIR=$(mktemp -d)`) moves all persistence to that directory.

Source: `apps/tolkin-cli/src/ledger.rs`, function `data_dir`. The function
checks `TOLKIN_DATA_DIR` (and the pre-rename alias `TOKLER_DATA_DIR`) before
falling back to `directories::ProjectDirs`.

### Consent at onboarding

The ledger writes nothing without an explicit consent step. `tolkin init`
records two consent flags in `config.toml`: `consent_ledger` (enables the
ledger) and `consent_log_ingestion` (enables reading local session logs). Both
default to false. The `append` function in `ledger.rs` is a silent no-op
unless `consent_ledger` is true in the loaded config.

Source: `apps/tolkin-cli/src/ledger.rs`, function `append` (line guard on
`cfg.consent_ledger`); `apps/tolkin-cli/src/commands/init.rs`.

### Deleting ledger data

`tolkin stats --reset` deletes `ledger.jsonl` and the usage parse cache
(`usage-cache.json`) from the data directory. `config.toml` (consent state)
is preserved. After reset, all tier computations start from a clean slate.

Source: `apps/tolkin-cli/src/commands/stats.rs`, function `reset`.

## Log ingestion

### Opt-in, read-only

Log ingestion is disabled by default. It is enabled only when the user
explicitly consents during `tolkin init` (the `consent_log_ingestion` flag).

When enabled, the ingestion reader opens local agent session log files
read-only. It extracts token counts, timestamps, the `cwd` field (for project
attribution), and the model id from each logged API response. It never reads
or stores message content (the `message.content` array), user input, or tool
output text.

Sources:
- `apps/tolkin-cli/src/usage/claude_code.rs`: Claude Code session reader.
  The reader parses only `message.model`, `message.usage`, `cwd`, `timestamp`,
  and `requestId` from each JSONL record.
- `apps/tolkin-cli/src/usage/types.rs`: the module-level privacy header
  states verbatim "keeps only token counts and timestamps, and never touches
  message content."
- `apps/tolkin-cli/src/usage/codex.rs`: Codex session reader; same posture.

### Which sources are ingested

Claude Code session logs from `~/.claude/projects/` (parent sessions and
subagent streams), and Codex session logs from `~/.codex/sessions/`. Both
paths are resolved from the home directory; `TOLKIN_HOME_DIR` overrides the
home root (a test seam used by the test harness to seed synthetic logs).

Source: `apps/tolkin-cli/src/usage/mod.rs`, function `default_dirs` and
`home_root`.

## Environment variables that disable persistence

- `CI=true`: disables all ledger writes. The GitHub Action sets this
  explicitly via `TOLKIN_NO_LEDGER: "1"` in the action steps, ensuring the
  action leaves no local state.
- `TOLKIN_NO_LEDGER=1` (any truthy value): disables all ledger writes.
  `"0"` and `"false"` are treated as not-set.
- `TOKLER_NO_LEDGER=1`: pre-rename alias; same effect.
- `TOLKIN_DATA_DIR=<path>`: relocates all persistence to the specified
  directory. Used in tests as `TOLKIN_DATA_DIR=$(mktemp -d)`.
- `TOLKIN_HOME_DIR=<path>`: test seam that overrides the home directory for
  log ingestion path resolution. Not used during normal operation.

Source: `apps/tolkin-cli/src/ledger.rs`, functions `disabled_by_env` and
`data_dir`.

## What the GitHub Action sends

The GitHub Action (`distribution/action/action.yml`) runs `tolkin project .
--json` in the repository checkout, builds a markdown report from the JSON
output, and posts it as a PR comment via the GitHub API using the job's
built-in `GITHUB_TOKEN`. No token data, file contents, or analysis results
leave the GitHub Actions runner to any external service. The PR comment
contains the same summary a developer would see by running `tolkin project`
locally: load-profile totals, heaviest files, identified savings range, and
the honesty tier labels. The action always sets `TOLKIN_NO_LEDGER: "1"`, so
the run leaves no local state on the runner.

Source: `distribution/action/action.yml` (the `TOLKIN_NO_LEDGER: "1"` env
line in the "Run tolkin against HEAD" step; the `gh api` call that posts the
PR comment; no external HTTP endpoints beyond the GitHub API).
