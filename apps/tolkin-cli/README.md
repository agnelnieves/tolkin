# tolkin

Tolkin CLI: a privacy-first AI token analyzer. This crate is the command-line entry point for Tolkin. It path-depends on `tolkin-core` and exposes subcommands for counting, visualizing, auditing, costing, redacting, drift detection, comparison, and running an MCP server. See `apps/tolkin-web/PLAN.md` for the broader product plan.

```
cargo build --release
./target/release/tolkin --help
./target/release/tolkin count my-prompt.md
```
