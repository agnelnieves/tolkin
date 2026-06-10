# tolkin-cli

Privacy-first AI token analyzer CLI. Count, audit, and optimize prompt and MCP token costs across OpenAI, Anthropic, and Gemini. Everything runs fully local: no telemetry, no uploads, nothing leaves your machine.

## Install

```sh
npx tolkin-cli --help
# or
bunx tolkin-cli --help
# or install globally (installs the `tolkin` command)
npm i -g tolkin-cli
```

The `tolkin-cli` package is a thin launcher exposing the `tolkin` command. The actual binary ships in a per-platform package (`tolkin-darwin-arm64`, `tolkin-darwin-x64`) pulled in automatically as an optional dependency.

## Commands

| Command | What it does |
| --- | --- |
| `tolkin count <FILE>` | Count tokens in a file or stdin. `--all` compares providers. |
| `tolkin compare <FILE>` | Side-by-side token counts across providers. |
| `tolkin viz <FILE>` | Visualize token boundaries (count plus estimate band for Claude). |
| `tolkin audit <FILE>` | Run the rules engine: ranked findings with savings estimates. |
| `tolkin redact <FILE>` | Strip secrets from input. Runs before anything else. |
| `tolkin cost <FILE>` | Estimate provider cost for an input. |
| `tolkin mcp <CONFIG>` | Analyze an MCP config: tool-definition token cost and CLI-swap savings. |
| `tolkin drift <FILE>` | Compare the same input across tokenizer versions (encoding drift). |
| `tolkin scan` | Discover your local agent configs (Claude, Cursor, Codex, VS Code, Zed, and more) and report MCP token costs, CLI-swap savings, instruction-file weight, and shell secret hygiene. Read-only. |
| `tolkin project [DIR]` | Repo-wide audit: walks a repository (gitignore-aware), splits agent-context weight by load profile (always vs on-invocation vs on-demand), ranks the heaviest skill/command/instruction files, flags secrets, and totals reclaimable tokens. `--fail-on high` for CI. |

Use `-` as the file argument to read stdin, and `--json` for machine-readable output where supported. Run `tolkin <command> --help` for full flags.

```sh
echo "hello world" | tolkin count -
tolkin audit prompt.md
tolkin mcp ~/.config/claude/claude_desktop_config.json
tolkin scan
```

## Supported platforms

macOS arm64 (Apple Silicon) and macOS x64 (Intel). Linux and Windows builds are coming.

## License

MIT
