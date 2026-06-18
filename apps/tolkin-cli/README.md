# tolkin

Tolkin CLI: a privacy-first AI token analyzer. This crate is the command-line entry point for Tolkin. It path-depends on `tolkin-core` and exposes subcommands for counting, visualizing, auditing, costing, redacting, drift detection, comparison, and running an MCP server. See `apps/tolkin-web/PLAN.md` for the broader product plan.

```
cargo build --release
./target/release/tolkin --help
./target/release/tolkin count my-prompt.md
```

## For agents

Every TUI surface keeps a non-TTY equivalent. Scripts and agents should use these contracts instead of driving the dashboard:

| Command | Output |
|---|---|
| `tolkin stats --json --global` | machine-wide stats as JSON (tiers, advisories, cache) |
| `tolkin stats --json` | the same, scoped to the current project |
| `tolkin stats --compact` | one static dashboard frame on stdout (no raw mode, pipe-safe) |
| `tolkin project --json` | the project scan report as JSON |
| `tolkin mcp --json` | the MCP probe report as JSON |
| `tolkin update` | checks the registry once and prints the exact upgrade command for the detected install channel |

Exit codes and stdio behavior:

- Bare `tolkin` without a TTY exits `2` with a usage error on stderr. It never blocks waiting for input.
- `tolkin stats --tui` under non-TTY stdio fails fast with an error naming the terminal requirement instead of entering raw mode.
- `tolkin stats --compact` writes exactly one frame to stdout and exits `0`.

Environment switches:

- `NO_COLOR`: forces the mono theme everywhere (any value).
- `TOLKIN_REDUCED_MOTION=1`: disables all dashboard animation; every surface renders at rest.
- `TOLKIN_THEME`: picks the dashboard theme (`tolkin-dark`, `tolkin-light`, `terminal`, `mono`), overriding the persisted config choice.
- `TOLKIN_DATA_DIR`: overrides the ledger and config directory (useful for hermetic test runs).
- `CI=true` or `TOLKIN_NO_LEDGER=1`: disables all state writes.

The interactive dashboard's help overlay (`?`) lists the same contracts.

## Upgrading

Always pair the two brew commands; third-party taps only refresh on `brew update`,
so `brew upgrade` alone can report "already installed" minutes after a release:

```sh
brew update && brew upgrade tolkin   # Homebrew installs
npm update -g @tolkin/cli             # npm installs
```

Not sure which channel installed tolkin? `tolkin update` detects it and prints
the exact command.
