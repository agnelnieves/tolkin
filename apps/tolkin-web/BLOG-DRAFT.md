# Blog post draft notes: Tolkin

This blog post will be posted on the `agnel-website` package instead, just so we can have this file as context later for the agent whenever I want to create the actual blog post.

Working notes only. Not a post yet. When drafting for real, the post goes to `apps/web/src/content/blog/` with full frontmatter and must follow the SEO/AEO rules in the root `CLAUDE.md` (answer-first opening, 120-160 char excerpt, internal links, BlogPosting JSON-LD is automatic). Two hard rules for the final post: never link the private source repo, and no em-dashes or en-dashes anywhere.

Companion file: `apps/tolkin-web/LESSONS.md` is the "what went wrong" log. Mine it for the post's friction beats and honest moments. `apps/tolkin-web/PROGRESS.md` is the canonical log; every claim in the post should trace to an entry there.

Updated 2026-06-11, post-launch: the product shipped as tolkin (after the second rename), the blind-research review happened, four more waves shipped (0.9.1 through 0.12.0), and the public repos, the first binary release, and the Homebrew tap are live. The notes below supersede the pre-rename draft.

## Working title candidates

- Tolkin: measuring the AI token tax before you pay it
- I let two researchers who never saw my code review my product (they converged on what I built)
- Your MCP servers are eating your context window. Here is the receipt.
- The benchmark that measures other people's benchmarks
- Claude Code was hiding a third of my spend in subagent transcripts

## Story arc (one line each)

- The problem: agent setups burn hundreds of thousands of input tokens per task and most of it is structural (standing context, tool-definition bloat, pretty-printed JSON, cold caches).
- The bet: an analyzer that measures instead of claims, privacy-first (nothing leaves the machine), advisor not rewriter.
- Research first: 8 parallel research agents mapped tokenizers, compression literature, MCP overhead, redaction, pricing; one decisive plan came out of it.
- Naming saga, twice: Tokenist to Tokenly to Tokle to Tokler (npm typosquat filter rejected bare "tokler" vs howler, hence the -cli suffix), then the owner renamed to tolkin at 0.9.0, deliberately before any public URL escaped. Lesson: rename cost compounds with every distribution surface.
- Architecture: one Rust core (rules, MCP analyzer, cost, redactor) compiled to WASM for the browser and linked natively into the CLI; tokenization stays platform-native; the core never tokenizes.
- The wedge: paste any agent's MCP config (or now a server's real tools/list manifest) and get the token cost with CLI-swap and slim snippets.
- The measurement spine: a local consented ledger, opt-in ingestion of agent session logs (token counts and timestamps only, never content), and a three-tier savings vocabulary (identified = advisory estimate, realized = measured structure with estimated frequency, measured = ground truth) that nothing else in this space has.
- The reconciliation ritual: every ground-truth number ships only after an independent reimplementation reproduces it field for field over real logs, run twice. It caught real bugs every time it ran.
- The review: two researches commissioned with zero repo knowledge converged on the architecture the product already had, validated the two deliberate rejections (no runtime proxy, no output-policy product), and found the two places it lagged (cache analysis, CI deltas). Blind convergence as design validation is a section on its own.
- The cache layer: tolkin's consented logs already contained everything needed to compute prompt-cache health locally (hit rate, write churn, the 5m vs 1h TTL counterfactual at marginal rates); nobody else in the space can compute TTL economics from ground truth.
- The subagent discovery: the cache reconciliation noticed the logs were not growing where they should; Claude Code nests subagent transcripts one directory deeper than anyone reads, and ingesting them surfaced a third more spend (the single best "measurement finds money" beat).
- The delta gate: a PR comment that says "always-loaded context is 7,453 tokens" gets muted; one that says "+4,092 tokens, +54.9 percent of base" gets acted on; the dogfood PR proved it live.
- The auditor position: the benchmark vendors comparator tools verbatim, runs them on declared fixtures, and publishes claimed versus measured with statuses and reasons; it has now measured four ecosystem claims well below their headlines and shipped one forensic not-runnable verdict.
- Adversarial review as a ritual: every ground-truth wave got a second agent with orders to break it; three reviewers confirmed five breaks (a misleading churn headline, a wrong toolset provenance note, a missing plain-output hint, drifted gap-semantics docs, and a benchmark figure attributed to a README that never claimed it); every one was fixed with a regression pin before shipping. The fifth one is the thesis: the honesty machinery polices its own footnotes.
- Distribution: six npm packages via OIDC trusted publishing (no stored tokens), agent skills, a Claude Code plugin, a composite GitHub Action with budget gates, and now a Homebrew tap whose formula was validated with brew install, test, and audit before the tap repo even existed.
- The launch mechanics people will like: the public repo is a fresh single-commit snapshot (no private history), the first release was cut from the same CI artifacts npm shipped, and the formula's checksums were substituted in URL order by a script that an adversarial pass later forced to become version-agnostic.
- Privacy as a feature: PRIVACY.md maps every claim to the file implementing it, including the honest disclosure that ledger records carry the project's absolute path.

## Numbers worth citing in the final post (verify against PROGRESS.md at draft time)

- Timeline: scaffold to 0.12.0 public launch in three days (2026-06-09 to 2026-06-11); versions 0.9.1, 0.10.0, 0.11.0, 0.12.0 shipped in one engagement day.
- Tests at launch: 246 CLI across 7 suites plus 121 core; the skill schema drift lint runs 10 checks in CI and was proven by planting a fake key and watching it fail.
- This machine's measured ground truth: 97.5 percent cache hit rate; after the subagent fix, sessions went 183 to 481 (183 working sessions plus 298 subagent streams) and measured cost rose by $330.96; every observed 5-minute cache write was subagent traffic the old walk could not see.
- TTL counterfactual (advisory estimate computed from ground-truth gaps): the 1h TTL wins, $50.19 vs $50.57 once subagent streams are counted; the break-even is 1.9 x W1 < 1.15 x W5 in marginal write tokens (write rate minus the 0.1x read you would have paid anyway).
- The dogfood delta row: +4,092 always-loaded tokens, +54.9 percent of base, rendered live in a PR comment by the rebuilt audit workflow.
- Model mix on this machine: the frontier model carries 94.0 percent of priced spend; output is 15.3 percent of the priced bill (the measured ceiling for any output-side compression tool).
- Claimed versus measured: repomix --compress claims about 70 percent, measures 45.47 on a declared corpus; cavemem measures 11.76 percent (the research-circulated 46 percent appears in no upstream README; its README claims about 75 percent for prose); caveman-shrink measures 11.72 percent lossy and 1.89 percent on manifests against its ecosystem's 65 percent headline; notion-slim is not-runnable (Windows-only binaries in the npm tarball despite README claims, zero releases, closed-source transform) but the full Notion manifest measures 15,698 tokens against the catalog's 26,000 estimate.
- GitHub MCP, measured: v1.2.0 registers 43 default tools at exactly 8,175 tokens (o200k_base, tokenized manifest) against the 40,000 representative catalog figure from the all-toolsets era; the slim variant (repos,issues) measures 4,953, a measured diff of 3,222.
- Cost calculator honesty: the old default total was 97 percent fabricated output volume (a price ratio reused as a volume ratio); the default is now input-side only with the estimate as a labeled opt-in.
- Gemini cached tokens are published at 10 percent of base input across the 2.5 family ($0.125 / $0.03 / $0.01 per MTok) and the analyzer's warm multipliers now derive from the pricing table instead of hardcodes that contradicted it.
- Six npm packages at 0.12.0 plus a live Homebrew tap; the brew path was proven end to end on launch night including Homebrew's new third-party tap trust prompt.

## Friction beats (mine LESSONS.md; the post's honesty section)

- The naming saga, twice; rename before the URLs escape.
- The orchestrator's shell drifted into agent worktrees and merged onto the wrong branch, twice in one session; verify pwd and branch before any merge.
- The npm verify step failed a healthy release because win32 lost a four-second registry replication race against its own successful publish; every required check now retries.
- CI's clippy is newer than local clippy; a lint that does not exist locally can fail main.
- dirs::home_dir() on Windows resolves through the known-folder API and ignores HOME and USERPROFILE entirely; integration tests needed a TOLKIN_HOME_DIR seam.
- The homebrew release job's first soft run failed on artifact names (bare target vs tolkin-prefixed), catching the bug before the job ever went strict.
- The secrets-wired-but-wallet-empty beat: both keyed workflows authenticated and reached the API on the first try, then failed identically on a billing 400; plumbing proven, credits pending.
- The TTL default regression incident (anthropics/claude-code#46829) as context: 1h usage in the wild may be lower than logs imply.

## Reference files (the full context for whoever drafts this)

- apps/tolkin-web/PLAN.md: original architecture and phasing, the source of truth.
- apps/tolkin-web/IMPROVEMENTS.md: the second plan (measurement, dashboard, benchmark, distribution) with the decisions never to relitigate in section 4.
- apps/tolkin-web/REVIEW-FINDINGS.md: the blind-research cross-reference review that drove the I6/I7 waves; the prompt-caching deep dive is its centerpiece.
- apps/tolkin-web/EXECUTION-PROMPT-I6.md: the orchestration prompt for the four-wave engagement (the agent-operated development section's primary artifact).
- apps/tolkin-web/PROGRESS.md: the canonical work log, bottom-up for the latest state.
- apps/tolkin-web/LESSONS.md: the friction log.
- packages/tolkin-core/crates/core/src/: pricing.rs, cost.rs, redact.rs, mcp.rs, mcp_tools.rs, audit.rs, format.rs (the engine).
- apps/tolkin-cli/src/: commands/, usage/ (ingestion incl. subagents), cache_analysis.rs, advisories.rs, tiers.rs, scan/ (the measurement spine).
- apps/tolkin-cli/benchmarks/: methodology.md, RESULTS.md, fixtures incl. vendored manifests with provenance (the auditor story).
- .github/workflows/: tolkin-publish.yml (incl. homebrew-release), tolkin-ci.yml (incl. the drift lint), tolkin-audit.yml (delta gates), tolkin-bench.yml (incl. the BYOK scored dispatch), tolkin-drift.yml.
- Public surfaces: github.com/agnelnieves/tolkin (skills, plugin, action, PRIVACY.md, releases), github.com/agnelnieves/homebrew-tolkin (the tap), npm tolkin-cli plus five platform packages, the /bench page.

## Angle reminders

- Written-not-generated voice; the no-dash rule exists for this reason.
- Describe the codebase generically (a Rust core alongside a Next.js app); no private repo URLs.
- The agent-operated build process (parallel worktree sub-agents, gates run by the orchestrator on the combined tree, adversarial reviewers with orders to break things, the PROGRESS ritual) is itself a story worth a full section; the five confirmed-and-pinned breaks are its proof.
- The three-tier vocabulary and the denominator convention (every percentage names its base) are the differentiators to explain early; the review found no equivalent anywhere in the space.
- Cite the honest-limits research (the March 2026 RCT: aggressive compression can raise total cost) when discussing savings claims; all tolkin claims are input-token bounded.
- The blind-research convergence is the strongest external validation available: lead with it or close with it.
