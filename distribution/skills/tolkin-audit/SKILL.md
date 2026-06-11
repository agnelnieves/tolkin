---
name: tolkin-audit
description: |
  Tolkin repo-wide audit skill. Use when the user asks to audit agent-context token
  costs, analyze CLAUDE.md / AGENTS.md weight, review skill or command file sizes,
  check for reclaimable tokens, understand the repo load profile, or reduce what gets
  sent to the model on every session. Also triggers on: "how heavy is my context",
  "what is my token footprint", "audit my agent files", "tolkin project".
metadata:
  version: 0.12.0
---

# tolkin-audit

Run a repo-wide agent-context audit using Tolkin, interpret the findings, and propose
concrete edits ranked by severity.

## When this skill applies

Use this skill whenever the user wants to understand or reduce their repo's agent-context
token footprint: CLAUDE.md, AGENTS.md, skills, commands, MCP configs, or anything else
that gets loaded into the model context.

## Step 1: Run the audit

```bash
npx tolkin-cli@latest project . --json
# bunx works identically:
# bunx tolkin-cli@latest project . --json
```

If a local release binary is available (e.g. during CI dry-runs), set
`TOLKIN_BIN=/path/to/tolkin` and the action uses it. The skill always prefers
`npx tolkin-cli@latest` in end-user sessions.

The command walks the repo (gitignore-aware), classifies every agent-context file by
load profile, and emits a JSON report.

## Step 2: Parse the JSON output

Key fields from `tolkin project . --json`:

<!-- tolkin-schema: project --json -->
```json
{
  "root": "/abs/path/to/repo",
  "profiles": {
    "always":        { "tokens": <u64>, "files": [...] },
    "on_invocation": { "tokens": <u64>, "files": [...] },
    "on_demand":     { "tokens": <u64>, "files": [...] },
    "docs":          { "tokens": <u64>, "files": [...] }
  },
  "mcp_configs": [
    { "path": "<str>", "servers": <u64>, "cold_tokens": <u64> }
  ],
  "heaviest": [
    { "path": "<str>", "tokens": <u64>, "findings": <u64>,
      "savings_min": <u64>, "savings_max": <u64> }
  ],
  "findings_by_rule": [
    { "rule": "<str>", "count": <u64>,
      "savings_min": <u64>, "savings_max": <u64> }
  ],
  "secret_files": [
    { "path": "<str>", "secret_count": <u64>, "kinds": ["<str>"] }
  ],
  "totals": {
    "files_scanned": <u64>,
    "context_files": <u64>,
    "context_tokens": <u64>,
    "savings_min": <u64>,
    "savings_max": <u64>
  },
  "warnings": ["<str>"]
}
```

Load profile meanings:
- `always`: loaded on every session start (CLAUDE.md, AGENTS.md, MCP cold tokens,
  skill description frontmatter). This is the standing context cost.
- `on_invocation`: loaded when a skill or command is invoked (skill bodies,
  command files, codex prompts).
- `on_demand`: reference files pulled in mid-session.
- `docs`: root markdown files (README.md, etc.).

## Step 3: Interpret and prioritize

1. **Always-loaded tokens are the highest priority.** Every session pays this cost.
   Focus reduction efforts on `profiles.always.tokens` before anything else.
2. **Check `secret_files`.** Files listed here contain values that tolkin flagged as
   potential secrets (high-entropy strings, key-like patterns). These are reported
   under `secret_files`, not under `findings_by_rule`. Review each file and remove
   or redact the values before they reach the model on every session.
3. **Rank `findings_by_rule` by `savings_max` descending.** Rules that commonly fire:
   - `json-verbosity`: pretty-printed JSON in context; minify or externalize.
   - `sub-cache-threshold`: a file too small to cross the prompt-cache threshold
     (1024 tokens); consolidate with a peer file to unlock caching.
   - `html-content`: raw HTML in a context file; convert to Markdown to cut tokens.
   - `near-duplicate-paragraphs`: repeated blocks within the same file; deduplicate.
     Note: the audit runs per-file. It does not detect duplication across different
     files; cross-file consolidation requires manual review.
   - `stack-trace-verbosity`: long stack traces; trim to the first 5 frames.
   - `volatile-prefix`: a frequently-changing block early in the context that
     prevents prompt-cache hits; move dynamic content toward the end.
   Production-proven rules that also fire in some repos: `filler-phrases`,
   `html-content`. Experimental rules (higher false-positive risk) include
   `json-toon-candidate`, `repeated-instructions`, `verbose-role-description`,
   `excessive-few-shot`, `markdown-overhead`, `lost-in-the-middle`.
4. **Check `mcp_configs`** for high `cold_tokens`. If cold tokens are significant,
   recommend running `tolkin-slim` to get per-server slim snippets.

## Step 4: Propose concrete edits

For each finding, propose a specific edit:
- Which file, which section, what to remove or consolidate.
- Before/after token estimate (use `savings_min` to `savings_max` range from the JSON).
- Severity label (high/medium/low from `findings_by_rule`).

Always frame savings as **identified (Tier 1)**: advisory estimates based on the
current file content. They become realized (Tier 2) only after the edit is made and
the project is re-analyzed. All numbers are input-token estimates; output may vary.

Do not propose lossy rewrites (stripping semantic content) without explicit user
opt-in. Safe edits are: minification of embedded JSON, deduplication of repeated
paragraphs, splitting a single large file into smaller on-demand files, and removing
genuinely redundant boilerplate.

## Step 5: Verify

After the user applies edits, re-run:

```bash
npx tolkin-cli@latest project . --json
```

Compare `totals.context_tokens` before and after. The delta is the **realized savings
(Tier 2)** for this session. Report it as: "Realized savings: ~N tokens removed from
the always-loaded context (input-token estimate; output may vary)."

If the user has usage logs ingested via `tolkin stats`, Tier 3 (measured) savings
will appear there once enough sessions have accumulated. Do not conflate the tiers.
