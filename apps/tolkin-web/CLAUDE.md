# Tolkin Operating Guide for Claude

You are working on Tolkin, a privacy-first AI token analyzer. Before any substantive work, read `apps/tolkin-web/PLAN.md`. It is the source of truth for architecture, scope, and phasing.

## Required behaviors

1. **Read PLAN.md first.** Architectural decisions live there. Do not re-derive them.
2. **Update PROGRESS.md when a work unit completes.** Append a dated entry to the Log section. Update the Status and Workspace tables to reflect new state.
3. **Stay in scope per the current phase.** Phase 0 is scaffolding. Phase 1 is the deterministic core. Do not implement Phase 2 or later features ahead of Phase 1.
4. **Match the monorepo conventions in `/CLAUDE.md`.** Rust-first toolchain, bun, Biome, oxlint, tsgo, no em-dashes or en-dashes in any human-written content, no `Co-Authored-By` lines or AI attribution in commits, never link to the private GitHub repo from public-facing content.

## Tolkin architecture (binding decisions)

- Three workspaces: `packages/tolkin-core` (Rust + WASM), `apps/tolkin-cli` (Rust binary), `apps/tolkin-web` (Next.js 16).
- `packages/tolkin-core` is its own Cargo workspace with two crates: `tolkin-core` (rlib used by the CLI) and `tolkin-core-wasm` (cdylib via wasm-bindgen, built to `pkg/` via `wasm-pack`). No root Cargo workspace at the repo level.
- The Rust core is the single source of truth for the rules engine, MCP analyzer, cost calculator, secret redactor, and MinHash/SimHash. Tokenization is platform-native (JS libs in the browser, Rust crates in the CLI).
- Privacy posture: no localStorage, no IndexedDB, no Service Worker cache by default. Hybrid verification (Anthropic `count_tokens`, Gemini `countTokens`) is opt-in per session, with the user supplying their own API key into local memory only.
- CLI persistence posture: local-only, consented, resettable persistence. The CLI keeps a savings ledger (headline numbers only, never file contents or secret values) in the platform data dir, created only after onboarding consent, wiped by `tolkin stats --reset`, and disabled entirely when `CI=true` or `TOLKIN_NO_LEDGER=1`. Zero network egress, ever; the only sanctioned fetch remains the opt-in BYOK verify.
- For Claude, never render fabricated per-token visualization. Show count plus confidence band only.
- All savings claims are input-token-bounded. Always surface "input savings, output may vary."

## Naming

- npm scope (when public): `@tolkin`.
- WASM artifact package name: `tolkin-core-wasm` (unscoped pre-OSS).
- CLI binary: `tolkin`.

## Tooling

- Package manager: **bun**. Never `npm`/`pnpm`/`yarn`. `bun add`, `bun install --frozen-lockfile`, `bunx`.
- Linting: **Biome 2** at the root + **oxlint** as a CI speed gate. No Prettier, no ESLint.
- Type checking: **tsgo** (TypeScript 7 Go-based preview) via `@typescript/native-preview`.
- Rust: `cargo clippy --all-targets -- -D warnings`, `cargo deny check`. License allow-list in each workspace's `deny.toml` (mirrored from `apps/cli/deny.toml`).
- WASM build: `wasm-pack build --target web` produces `pkg/`, consumed as a workspace dependency by `apps/tolkin-web`.

## Tracking convention

- `apps/tolkin-web/PLAN.md`: source of truth for the architecture.
- `apps/tolkin-web/PROGRESS.md`: canonical work log. Update at the end of every work unit.
- `apps/tolkin-web/AGENTS.md`: agent-agnostic version of this file. Keep the two in sync.
- TaskCreate/TaskUpdate may be used for in-session tracking but never replaces PROGRESS.md, which persists across sessions.

## Working in adjacent Tolkin directories

`apps/tolkin-cli` and `packages/tolkin-core` do not carry their own CLAUDE.md. If you start a session in either, the root `/CLAUDE.md` loads automatically; treat this file (and PLAN.md) as the next required read.
