# Vercel Deploy Migration: tolkin-web

## Context

The tolkin-web Next.js app moved from the `agnelnieves/agnelweb` monorepo into
the standalone `agnelnieves/tolkin` monorepo. The Vercel project keeps the same
name and domain. Only the Git source needs to be re-pointed.

---

## One-Time Migration Steps

### 1. Re-point Git source in Vercel

1. Open the Vercel dashboard and navigate to the **tolkin-web** project (or
   whatever the project is named; it serves tolkin.dev).
2. Go to **Settings > Git**.
3. Under "Connected Git Repository", click **Disconnect** (or "Change").
4. Connect to `agnelnieves/tolkin`.
5. Save.

### 2. Verify project settings

| Setting | Value |
|---|---|
| Root Directory | `apps/tolkin-web` |
| Framework Preset | Next.js |
| Build Command | `bun run build` |
| Output Directory | `out` |
| Install Command | `bun install` (run from repo root) |
| Node Version | match `.nvmrc` in repo root |

Set "Root Directory" to `apps/tolkin-web` so Vercel resolves the workspace from
the monorepo root. The install command runs at the repo root, which lets Bun
resolve workspace packages (`@tolkin/core-wasm`, `@repo/tsconfig`).

### 3. Environment variables

None required for the static export. The GA4 measurement ID (`G-8QPMMZHYKZ`)
is hardcoded in `src/components/analytics.tsx`. `NODE_ENV=production` is set
automatically by Vercel at build time, which enables the analytics snippet.

If you ever need to externalize the GA ID, add:

```
NEXT_PUBLIC_GA_ID=G-8QPMMZHYKZ
```

and update `analytics.tsx` to read `process.env.NEXT_PUBLIC_GA_ID`.

### 4. DNS

Do not touch DNS records. The Vercel project retains its domain assignment
after the Git re-point. Domains follow the project, not the repository.

---

## Pre-build Dependency: WASM Package

`apps/tolkin-web` depends on `@tolkin/core-wasm` (workspace package built from
`packages/tolkin-core`). Vercel's standard install step does not run
`wasm-pack`; the compiled WASM artifacts (`pkg/`) must be committed to the repo
or built in a custom build command.

Current approach: the `pkg/` directory is committed. Verify this before each
major Rust change:

```sh
cd packages/tolkin-core
bun run build   # wasm-pack build crates/wasm --target web --out-dir ../../pkg --release
git add ../../pkg
git commit -m "chore(wasm): rebuild pkg artifacts"
```

If `wasm-pack` is not present on the Vercel build image, the install command
must be extended:

```
cargo install wasm-pack --locked && bun install
```

Set this as the **Install Command** in Vercel project settings.

---

## Verification After Re-point

1. Trigger a new deploy from the Vercel dashboard (or push a commit to `main`).
2. Watch the build log. Expect:
   - `bun install` resolves workspace deps with no changes.
   - `next build` (Turbopack) compiles all routes as static (`○`).
   - Output written to `apps/tolkin-web/out/`.
3. Open the production URL. Confirm the landing page loads and the analyzer
   route (`/analyzer/`) is reachable.
4. Check DevTools > Network. No 500s or missing WASM fetches.

---

## Local Smoke Test (run before pushing)

```sh
# From repo root
bun install

# Build WASM (if pkg/ is stale)
cd packages/tolkin-core && bun run build && cd ../..

# Build web
cd apps/tolkin-web && bun run build

# Serve locally
bunx serve out -p 3000
```

Expected: all routes listed in the build summary resolve without errors.
