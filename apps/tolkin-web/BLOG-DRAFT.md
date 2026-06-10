# Blog post draft notes: Tokler

This blog post will be posted on the `agnel-website` package instead, just so we can have this file as context later for the agent whenever I want to create the actual blog post.

Working notes only. Not a post yet. When drafting for real, the post goes to `apps/web/src/content/blog/` with full frontmatter and must follow the SEO/AEO rules in the root `CLAUDE.md` (answer-first opening, 120-160 char excerpt, internal links, BlogPosting JSON-LD is automatic). Two hard rules for the final post: never link the private source repo, and no em-dashes or en-dashes anywhere.

Companion file: `apps/tokler-web/LESSONS.md` is the "what went wrong" log. Mine it for the post's friction beats and honest moments. Future entries get appended there as we go.

## Working title candidates

- Tokler: measuring the AI token tax before you pay it
- I built a token analyzer that runs entirely in your browser (and your terminal)
- Your MCP servers are eating your context window. Here is the receipt.

## Story arc (one line each)

- The problem: agent setups burn ~400K input tokens per PR and most of it is structural waste (re-sent context, tool-definition bloat, pretty-printed JSON).
- The bet: an analyzer that measures instead of claims, privacy-first (nothing leaves the machine), advisor not rewriter.
- Research first: 8 parallel research agents mapped tokenizers, compression literature, MCP overhead benchmarks, secret redaction, pricing; one decisive plan came out of it.
- Naming saga: Tokenist (taken) to Tokenly to Tokle to Tokler; npm typosquat filter rejected bare "tokler" as too similar to howler, hence tokler-cli.
- Architecture: one Rust core (rules, MCP analyzer, cost, redactor) compiled to WASM for the browser and linked natively into the CLI; tokenization stays platform-native.
- The wedge: paste any agent's MCP config, get the token cost of its tool definitions plus CLI-swap and slim-profile recommendations with copy-pasteable snippets.
- Honesty as the moat: every Claude count labeled an estimate until verified, savings are input-token bounded, prices are date-stamped, experimental rules are badged.
- The drift story: Opus 4.7's tokenizer quietly emits up to 35 percent more tokens than 4.6; tokler drift makes that visible.
- Repo-wide audits: tokler project splits a repo's agent-context weight by load profile (always vs on-invocation vs on-demand), the number budget conversations actually need.
- Local discovery: tokler scan finds your MCP configs across 16 client locations, checks which CLI replacements are actually installed, and flags secrets exported in shell configs.
- Shipping: five npm platform packages (darwin arm64/x64, linux x64/arm64, windows pending) behind a 52-line launcher, published by CI via OIDC trusted publishing, no stored tokens.
- The pipeline detail people will like: a version gate makes ordinary pushes to main a no-op; a bump plus merge is a release.
- What is next (from IMPROVEMENTS.md): local savings ledger, real measured spend from agent session logs, a Ratatui dashboard, a public three-track benchmark, skills/plugin/action distribution.

## Numbers worth citing in the final post (verify against PROGRESS.md at draft time)

- 5 phases from scaffold to published in 2 days (2026-06-09 to 2026-06-10); commits 2e3e345 through the 0.3.0 merge.
- Core: 83 unit tests; CLI: 48; zero network egress except one opt-in BYOK verify call.
- CLI binary ~25.5 MB (17.5 MB is the embedded Gemma tokenizer).
- WASM core ~937 KB; web first-paint keeps parsers and tokenizers lazy.
- MCP example: GitHub MCP ~26-55K cold tokens vs ~500 for gh CLI discovery; slim GITHUB_TOOLSETS snippet saves ~20K.
- Audit catalog: 6 production-proven rules plus 6 experimental, each with citations.

## Reference files (the full context for whoever drafts this)

- apps/tokler-web/PLAN.md: original architecture and phasing, the source of truth.
- apps/tokler-web/IMPROVEMENTS.md: the second plan (measurement, dashboard, benchmark, distribution).
- apps/tokler-web/PROGRESS.md: the canonical work log, bottom-up for the latest state; every claim in the post should trace to an entry here.
- apps/tokler-web/HANDOFF.md: condensed context snapshot and decisions/gotchas list.
- apps/tokler-web/CLAUDE.md and AGENTS.md: the operating rules the project was built under (worth a paragraph in the post about agent-operated development).
- packages/tokler-core/crates/core/src/: pricing.rs, cost.rs, redact.rs, mcp.rs, audit.rs, format.rs (the engine the post describes).
- apps/tokler-cli/src/: commands/, tokenize/, scan/, project/, verify.rs (CLI surface).
- .github/workflows/: tokler-publish.yml, tokler-ci.yml, tokler-audit.yml, tokler-drift.yml (the automation story).
- Published packages: tokler-cli, tokler-darwin-arm64, tokler-darwin-x64, tokler-linux-x64, tokler-linux-arm64 on npm.

## Angle reminders

- Written-not-generated voice; the no-dash rule exists for this reason.
- Describe the codebase generically (a Rust core alongside a Next.js app); no private repo URLs.
- The agent-operated build process (parallel sub-agents, gates, PROGRESS ritual) is itself a story worth a section.
- Cite the honest-limits research (the March 2026 RCT: aggressive compression can raise total cost) when discussing savings claims.
