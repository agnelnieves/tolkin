# Tolkin Operating Guide for Agents

You are working on Tolkin, a privacy-first AI token analyzer. Before any substantive work, read `apps/tolkin-web/PLAN.md`. It is the source of truth for architecture, scope, and phasing.

This file is the agent-agnostic version of `CLAUDE.md`. Both are kept in sync. Different agents look for different filenames; treat the contents as authoritative either way.

## Required behaviors

1. **Read PLAN.md first.** Architectural decisions live there. Do not re-derive them.
2. **Update PROGRESS.md when a work unit completes.** Append a dated entry to the Log section. Update the Status and Workspace tables to reflect new state.
3. **Stay in scope per the current phase.** Phase 0 was scaffolding. Phase 1 is the deterministic core. Do not implement Phase 2 or later features ahead of Phase 1.
4. **Follow the monorepo conventions.** Rust-first toolchain, bun, Biome, oxlint, tsgo, no em-dashes or en-dashes in any human-written content. No AI attribution lines in commits.

## Workspace layout

```
.
├── apps/
│   ├── tolkin-cli/        Rust binary + npm distribution
│   └── tolkin-web/        Next.js 16 site (static export)
├── packages/
│   ├── tolkin-core/       Rust workspace: tolkin-core (rlib) + tolkin-core-wasm (cdylib)
│   └── tsconfig/          @repo/tsconfig shared TS presets
├── distribution/
│   ├── action/            Composite GitHub Action
│   ├── homebrew/          Homebrew Formula template
│   ├── skills/            Four agent skills
│   ├── PRIVACY.md         Privacy posture
│   ├── LICENSE            MIT
│   └── README.md          Public-facing distribution overview
└── .github/workflows/     Six tolkin-*.yml workflows
```

## Tolkin architecture (binding decisions)

- Three workspaces: `packages/tolkin-core` (Rust + WASM), `apps/tolkin-cli` (Rust binary), `apps/tolkin-web` (Next.js 16).
- `packages/tolkin-core` is its own Cargo workspace with two crates: `tolkin-core` (rlib used by the CLI) and `tolkin-core-wasm` (cdylib via wasm-bindgen, built to `pkg/` via `wasm-pack`). No root Cargo workspace at the repo level.
- The Rust core is the single source of truth for the rules engine, MCP analyzer, cost calculator, secret redactor, and MinHash/SimHash. Tokenization is platform-native (JS libs in the browser, Rust crates in the CLI).
- Privacy posture: no localStorage, no IndexedDB, no Service Worker cache by default. Hybrid verification (Anthropic `count_tokens`, Gemini `countTokens`) is opt-in per session, with the user supplying their own API key into local memory only.
- CLI persistence posture: local-only, consented, resettable persistence. The CLI keeps a savings ledger (headline numbers only, never file contents or secret values) in the platform data dir, created only after onboarding consent, wiped by `tolkin stats --reset`, and disabled entirely when `CI=true` or `TOLKIN_NO_LEDGER=1`. See `distribution/PRIVACY.md`.
- All savings claims are input-token-bounded. Always surface "input savings, output may vary."

## Naming

- npm scope: `@tolkin`.
- Public wrapper package: `@tolkin/cli`. Platform binaries: `@tolkin/<platform>-<arch>`.
- WASM artifact package name: `@tolkin/core-wasm` (workspace-internal, `private: true`).
- CLI binary: `tolkin`.

## Tooling

- Package manager: **bun**. Never `npm`, `pnpm`, or `yarn`. `bun add`, `bun install --frozen-lockfile`, `bunx`.
- Linting: **Biome 2** at the root + **oxlint** as a CI speed gate. No Prettier, no ESLint.
- Type checking: **tsgo** (TypeScript 7 Go-based preview) via `@typescript/native-preview`.
- Rust: `cargo clippy --all-targets -- -D warnings`, `cargo deny check`.
- WASM build: `wasm-pack build crates/wasm --target web --out-dir ../../pkg --release` from `packages/tolkin-core/`.

## Tracking convention

- `apps/tolkin-web/PLAN.md`: source of truth for the architecture.
- `apps/tolkin-web/PROGRESS.md`: canonical work log. Update at the end of every work unit.
- Root `CLAUDE.md` covers the whole monorepo. `apps/tolkin-cli` and `packages/tolkin-core` do not carry their own.
- In-session task lists are fine, but they never replace PROGRESS.md.
