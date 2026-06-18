# Tolkin Operating Guide for Claude

You are working on Tolkin, a privacy-first AI token analyzer. Before any substantive work, read `apps/tolkin-web/PLAN.md`. It is the source of truth for architecture, scope, and phasing.

## Required behaviors

1. **Read PLAN.md first.** Architectural decisions live there. Do not re-derive them.
2. **Update PROGRESS.md when a work unit completes.** Append a dated entry to the Log section. Update the Status and Workspace tables to reflect new state.
3. **Stay in scope per the current phase.** Phase 0 was scaffolding. Phase 1 is the deterministic core. Do not implement Phase 2 or later features ahead of Phase 1.
4. **Follow the monorepo conventions.** Rust-first toolchain, bun, Biome, oxlint, tsgo, no em-dashes or en-dashes in any human-written content (this applies to blog posts, guides, READMEs, commit messages, and code comments). Hyphens inside compound words and code identifiers are fine. No `Co-Authored-By` lines or AI attribution in commits.

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
│   ├── skills/            Four agent skills (tolkin-audit, tolkin-slim, tolkin-optimize, tolkin-cache)
│   ├── PRIVACY.md         Privacy posture
│   ├── LICENSE            MIT
│   └── README.md          Public-facing distribution overview
└── .github/workflows/     Six tolkin-*.yml workflows (ci, publish, audit, bench, drift, action-dryrun)
```

## Tolkin architecture (binding decisions)

- Three workspaces: `packages/tolkin-core` (Rust + WASM), `apps/tolkin-cli` (Rust binary), `apps/tolkin-web` (Next.js 16).
- `packages/tolkin-core` is its own Cargo workspace with two crates: `tolkin-core` (rlib used by the CLI) and `tolkin-core-wasm` (cdylib via wasm-bindgen, built to `pkg/` via `wasm-pack`). No root Cargo workspace at the repo level.
- The Rust core is the single source of truth for the rules engine, MCP analyzer, cost calculator, secret redactor, and MinHash/SimHash. Tokenization is platform-native (JS libs in the browser, Rust crates in the CLI).
- Privacy posture: no localStorage, no IndexedDB, no Service Worker cache by default. Hybrid verification (Anthropic `count_tokens`, Gemini `countTokens`) is opt-in per session, with the user supplying their own API key into local memory only.
- CLI persistence posture: local-only, consented, resettable persistence. The CLI keeps a savings ledger (headline numbers only, never file contents or secret values) in the platform data dir, created only after onboarding consent, wiped by `tolkin stats --reset`, and disabled entirely when `CI=true` or `TOLKIN_NO_LEDGER=1`. Sanctioned egress is limited to two tolkin-chosen, user-controlled endpoints (the opt-in BYOK verify; the npm-registry update check, explicit via `tolkin update` or once daily behind `consent_update_check`) plus one user-directed class: `tolkin mcp --probe` speaks the MCP handshake only to servers taken verbatim from the user's own config, per-server confirmed, refused in CI, manifests redacted and cached committably. The opt-in local-model layer (`tolkin optimize`) talks only to a loopback sidecar behind its own `consent_local_model` class; model output is labeled "model advisory (local)", never a tier, and every deterministic command stays byte-identical with or without a sidecar (enforced by tests/determinism.rs). See `distribution/PRIVACY.md`.
- All savings claims are input-token-bounded. Always surface "input savings, output may vary."

## Naming

- npm scope: `@tolkin`.
- Public wrapper package: `@tolkin/cli`. Platform binaries: `@tolkin/darwin-arm64`, `@tolkin/darwin-x64`, `@tolkin/linux-x64`, `@tolkin/linux-arm64`, `@tolkin/win32-x64`.
- WASM artifact package name: `@tolkin/core-wasm` (workspace-internal, `private: true`).
- CLI binary: `tolkin`.

## Tooling

- Package manager: **bun**. Never `npm`, `pnpm`, or `yarn`. `bun add`, `bun install --frozen-lockfile`, `bunx`.
- Linting: **Biome 2** at the root + **oxlint** as a CI speed gate. No Prettier, no ESLint.
- Type checking: **tsgo** (TypeScript 7 Go-based preview) via `@typescript/native-preview`.
- Rust: `cargo clippy --all-targets -- -D warnings`, `cargo deny check`. License allow-list in each workspace's `deny.toml`.
- Skill schema drift: `apps/tolkin-cli/scripts/check-skill-schemas.ts` asserts every JSON key documented in `distribution/skills/*/SKILL.md` exists in live `--json` output and that skill versions match Cargo.toml. Runs in tolkin-ci.
- WASM build: `wasm-pack build crates/wasm --target web --out-dir ../../pkg --release` from `packages/tolkin-core/`, producing `packages/tolkin-core/pkg/`. Consumed by `apps/tolkin-web` via `@tolkin/core-wasm: workspace:*`.

## Release flow

Version is synced across 13 carriers by `apps/tolkin-cli/scripts/bump-version.sh` (Cargo.toml, Cargo.lock, 6 npm package.json files, 4 SKILL.md files). After a bump lands on `main`, `.github/workflows/tolkin-publish.yml` runs:
1. Version gate (skip if registry already has `@tolkin/cli@<version>`).
2. Build platform binaries (matrix of 4-5 runners).
3. Publish 6 `@tolkin/*` packages via npm OIDC Trusted Publishing (platforms first, wrapper last so its `optionalDependencies` resolve).
4. Create the GitHub release on `agnelnieves/tolkin` (auth: `GITHUB_TOKEN`).
5. Update the Homebrew tap at `agnelnieves/homebrew-tolkin` (auth: `HOMEBREW_TAP_TOKEN`).

## Tracking convention

- `apps/tolkin-web/PLAN.md`: source of truth for the architecture.
- `apps/tolkin-web/PROGRESS.md`: canonical work log. Update at the end of every work unit.
- `apps/tolkin-web/AGENTS.md`: agent-agnostic version of `apps/tolkin-web/CLAUDE.md`. Keep the two in sync.
- This root `CLAUDE.md` covers the whole monorepo. `apps/tolkin-cli` and `packages/tolkin-core` do not carry their own.
- TaskCreate or TaskUpdate may be used for in-session tracking but never replaces PROGRESS.md.
