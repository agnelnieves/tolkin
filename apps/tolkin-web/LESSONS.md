# Tokler: what went wrong (and what we learned)

Running log of the rough moments. Honest, dated, brief. Future blog-post context. The polished post lives in `apps/web/src/content/blog/` (see `BLOG-DRAFT.md`); this file is the unvarnished source material.

Append a new entry per incident. Format: ISO date + one-line summary, then a `What happened`, `Why`, `Fix`, and (optional) `Lesson` block. Keep the lesson generalizable; the incident itself is concrete. No em-dashes anywhere.

---

## 2026-06-09: Naming saga ended at Tokler-cli

- **What happened:** Started as Tokenist (`tokenist.com` is an established 7-year-old crypto/fintech site, SEO collision). Renamed to Tokenly, then to Tokle, then to Tokler. npm let us register all the platform packages (`tokler-darwin-arm64`, `tokler-darwin-x64`, etc.) but blocked the bare `tokler` wrapper with E403: "Package name too similar to existing package howler."
- **Why:** npm's typosquat-protection filter is character-distance based and not consistent across siblings; suffixed names slip through, bare names that look like a popular package do not.
- **Fix:** Wrapper renamed to `tokler-cli`; installed binary stays `tokler`. `npx tokler-cli`, `bunx tokler-cli`, `npm i -g tokler-cli` all work; user experience is unchanged.
- **Lesson:** Check the npm registry before naming, AND test the bare-wrapper name with a dry-run publish before committing to assets. The brand can survive a tail (`-cli`) far better than a half-built product can survive a rename.

## 2026-06-09: Sub-agent connection died mid-Phase 1a

- **What happened:** The web tokenization agent ran for ~71 minutes (65 tool uses), produced five well-designed files (`src/lib/tokenize/*.ts`, the panel), then the API connection dropped before returning a summary. `package.json` looked unchanged afterwards.
- **Why:** Long-running agent + API blip. The work was on disk but the deps were misplaced.
- **Fix:** Diagnosed by reading the files the agent had left (the imports made the intent obvious). Re-ran the deps install manually; everything else was already done.
- **Lesson:** Trust-but-verify on long agent runs. The output on disk is the truth; the summary is just a postcard.

## 2026-06-09: `bun add --filter` put deps on the WRONG package.json

- **What happened:** `bun add --filter=tokler-web gpt-tokenizer @huggingface/transformers` added the deps to the ROOT `package.json` instead of `apps/tokler-web/package.json`. Build still passed because of workspace hoisting; the bug was invisible until reviewing the diff.
- **Why:** A subtle quirk of how Bun resolved the filter in this repo's workspace shape.
- **Fix:** Moved the deps into the right workspace `package.json`, re-ran `bun install`.
- **Lesson:** After any `bun add --filter`, `cat` both the workspace package.json AND the root to confirm the deps landed where they belong. Hoisting hides this.

## 2026-06-09: bpe-lite is Node-only; cl100k_base proxy is the right Claude approximator

- **What happened:** Original plan called for `bpe-lite` to approximate Claude tokens in the browser. It depends on `node:buffer` and `node:fs`. It does not bundle for the browser without a polyfill stack.
- **Why:** Library targets server runtimes; the part of it that approximates Claude is just `cl100k_base` under the hood.
- **Fix:** Dropped bpe-lite entirely. Both the CLI and the web use `cl100k_base` (via `tiktoken-rs` in Rust, `gpt-tokenizer/encoding/cl100k_base` in JS) and label every Claude count as a `~ estimate, +/-10%`. Exact counts are deferred to the opt-in Anthropic `count_tokens` hybrid.
- **Lesson:** When the SOTA approximation is a wrapper around something you already have, take the something you already have and label it honestly.

## 2026-06-09: wasm-pack rejected the cost calculator's bytecode

- **What happened:** WASM build failed with "Bulk memory operations require bulk memory" the first time. Later, in Phase 1c, it failed again with `i64.trunc_sat_f64_u` (nontrapping float-to-int) coming from the cost calculator's casts.
- **Why:** wasm-pack 0.15 ships an older `wasm-opt` that does not enable post-MVP features by default; modern Rust emits them.
- **Fix:** `package.metadata.wasm-pack.profile.release` set to `wasm-opt = ['-O', '--enable-bulk-memory', '--enable-nontrapping-float-to-int']`.
- **Lesson:** Pin and document wasm-opt feature flags up front. Two flags, one source of mystery failures avoided.

## 2026-06-09: WASM artifact and parent package.json name collision

- **What happened:** Both the parent `packages/tokler-core/package.json` and the wasm-pack output `pkg/package.json` claimed the same npm name (`tokler-core-wasm`). Bun resolved the workspace dep to the parent dir, which had no `main`/`exports`, so the import failed.
- **Why:** wasm-pack writes a complete npm package in `pkg/`; the parent dir is also a workspace member.
- **Fix:** The parent `package.json` re-exports `pkg/` via `main`/`types`/`exports`. The web imports `tokler-core-wasm`, Bun follows the parent's exports field into the built pkg.
- **Lesson:** When the build artifact wants to be its own npm package and the workspace also wants it to be one, only one of them gets the name. The parent forwards.

## 2026-06-10: npm OTP wall blocked CI publishes (and a non-numeric OTP error)

- **What happened:** First attempted publish from the workflow hit an OTP prompt; a follow-up local publish accidentally passed `--otp=XXXXXX` (placeholder text), which npm rejected with "fails to match the required pattern: /^\\d+$/, length must be 64 characters long."
- **Why:** Granular access tokens require 2FA OTPs for CI publishes; the 64-character "OTP" length means npm now treats the browser-auth response as the OTP, not a 6-digit code. Pasted placeholder text obviously matches neither.
- **Fix:** Browser-auth flow worked manually; long-term, switched the workflow to npm Trusted Publishing (OIDC, no stored tokens, no OTP rotation, no 2FA bypass risk).
- **Lesson:** If npm flags a granular access token as "security risk for CI, use Trusted Publishing instead," that is the answer. Take it.

## 2026-06-10: First-publish chicken-and-egg with OIDC trusted publishing

- **What happened:** The first publish workflow run on main succeeded for the three pre-existing packages (tokler-cli, tokler-darwin-arm64, tokler-darwin-x64 via OIDC), but failed for the brand-new `tokler-linux-x64` and `tokler-linux-arm64`. Then the verify step hard-required the linux versions and failed an otherwise successful release.
- **Why:** Trusted Publisher configuration lives on a package's npm settings page. A package that does not exist has no settings page. So the first publish of any new package cannot use OIDC.
- **Fix:** Two-part. (1) The two linux publish steps now `continue-on-error` so the wrapper still ships if they fail. (2) The verify step is strict on the wrapper and darwin packages and warning-only on the linux packages. Owner manually first-published the two linux packages with browser auth; subsequent releases flow automatically once Trusted Publishers are configured.
- **Lesson:** OIDC trusted publishing is the right destination, but every brand-new npm package needs one manual publish first. Plan for it.

## 2026-06-10: Sandbox revoked Documents access mid-session

- **What happened:** macOS TCC revoked the Claude Code session's access to `~/Documents` partway through; every shell and file tool returned EPERM, including bare `ls /Users/agnel/Documents`.
- **Why:** Likely a TCC re-prompt during a `/login` or model switch that was not granted.
- **Fix:** System Settings > Privacy & Security > Files and Folders > grant the terminal/Claude Code app access to Documents. Quitting and relaunching the terminal usually re-triggers the prompt.
- **Lesson:** A blocked tool that looks like a permissions bug usually is. The first guess on macOS should always be TCC.

## 2026-06-10: The naming saga continues, tokler -> tolkin

- **What happened:** With 0.8.0 live (five npm packages, a dashboard, a benchmark, a staged public repo), the owner renamed the product again: tokler -> tolkin. All six tolkin npm names were free; the rename landed as 0.9.0.
- **Why:** Owner branding call, made deliberately at the cheapest possible moment: before the public companion repo existed, before the blog post, and conveniently before tokler-win32-x64's first publish (which now simply never happens; tolkin-win32-x64 gets first-published instead).
- **Fix:** npm packages cannot be renamed in place. The cutover is: publish six new tolkin packages (each needing the by-now-familiar manual first publish before OIDC trusted publishing can take over), then deprecate the five live tokler packages with a pointer message. Repo-side: git mv + ordered content rename with historical logs excluded, env vars renamed with old-name fallbacks, the data dir migrates itself on first run, and old ledger records keep parsing through a serde alias (record schema bumped to v2).
- **Lesson:** Rename cost compounds with every distribution surface you ship. The same rename after the public repo, the action marketplace listing, and the blog post would have been a multi-week tail of redirects and stale docs. If a rename is coming, do it before the URLs escape. Also: one letter away from a famous trademark is a conscious risk, not an accident; it was flagged and accepted.

## Reusable patterns to call out in the eventual post

- The `<scope>-<package>` naming convention turns "name too similar" rejections into a non-event. Suffixes are not branding tax; they are insurance.
- "Run the local gate suite once before every phase" caught more issues than any review pass. The cost of a clean baseline is one minute; the cost of a regression untraceable to a commit is hours.
- Trust-but-verify on parallel agents: always read what landed before believing the summary. Files do not lie; summaries sometimes round.
- Privacy posture is a feature, not friction. Saying out loud "nothing leaves your browser" forced every downstream decision (no localStorage default, redaction first, hybrid opt-in only, ledger consented) and the codebase is simpler for it.
