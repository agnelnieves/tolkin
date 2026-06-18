# Contributing

We welcome contributions to Tolkin. Please follow the guidelines below.

## Getting started

Clone the repository and install dependencies:

```sh
git clone https://github.com/agnelnieves/tolkin.git
cd tolkin
bun install
```

## Building

The Tolkin monorepo contains three main workspaces: `packages/tolkin-core` (Rust + WebAssembly), `apps/tolkin-cli` (Rust binary), and `apps/tolkin-web` (Next.js 16 web UI).

### Rust core and CLI

From the root or within `apps/tolkin-cli`:

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

To build the WebAssembly module, run from `packages/tolkin-core`:

```sh
wasm-pack build --target web
```

### Web UI

From the root:

```sh
bun run build
bun run test
bun run typecheck
```

The web UI consumes the WASM module from `packages/tolkin-core/pkg/` as a workspace dependency.

## Testing

Run all tests:

```sh
cargo test          # Rust tests
bun run test        # TypeScript/JavaScript tests
```

## Code standards

This project enforces code style via Biome 2 and oxlint.

### Formatting and linting

```sh
bun run lint        # Run Biome lint and import sort
bun run format      # Format code with Biome
cargo clippy --all-targets -- -D warnings  # Lint Rust code
```

### Style guide

- No em-dashes or en-dashes. Use periods, commas, parentheses, colons, or sentence breaks instead.
- Hyphens are fine in compound words and code identifiers.
- Commit messages must not include `Co-Authored-By` or AI attribution lines.
- Keep commit messages concise and descriptive of the actual change.

## Filing an issue

If you find a bug or want to suggest a feature:

1. Check existing issues to avoid duplicates.
2. Use the appropriate template (`bug_report.md` or `feature_request.md` in `.github/ISSUE_TEMPLATE/`).
3. For bugs, include your Tolkin version (`tolkin --version`) and the steps to reproduce.
4. For security issues, see [SECURITY.md](SECURITY.md).

## Submitting a pull request

1. Create a feature branch: `git checkout -b your-feature-name`.
2. Make your changes and ensure tests pass and code is formatted.
3. Write a clear, concise PR summary (see `.github/PULL_REQUEST_TEMPLATE.md`).
4. Reference any related issues.
5. Ensure the PR description includes what changed and why.

Merges require all checks to pass and approval from a maintainer.

## Architecture

Before making substantial changes, read `apps/tolkin-web/PLAN.md`. It documents the binding architectural decisions and current phase scope.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
