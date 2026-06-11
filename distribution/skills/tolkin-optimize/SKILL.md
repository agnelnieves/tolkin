---
name: tolkin-optimize
description: |
  Tolkin full optimization loop skill. Use when the user wants a guided end-to-end
  token reduction pass: audit the repo, apply safe fixes, re-measure, and summarize
  with honest tier labels. Also triggers on: "optimize my context", "reduce my token
  footprint", "tolkin optimize loop", "full token audit and fix", "run tolkin and
  fix issues", "end-to-end token savings".
metadata:
  version: 0.15.1
---

# tolkin-optimize

Run the full Tolkin optimization loop: audit, apply safe fixes (with user confirmation),
re-measure, and summarize. Never apply lossy rewrites without explicit opt-in.

## Overview

This skill chains `tolkin-audit` and `tolkin-slim` into a single guided pass. The loop
is: audit -> propose -> confirm -> apply -> re-measure -> summarize.

## Step 1: Baseline audit

```bash
npx tolkin-cli@latest project . --json
# bunx works identically
```

Record the baseline numbers from `totals`:
- `context_tokens`: total tokens in agent context
- `savings_min` / `savings_max`: identified reclaimable range (Tier 1)
- `profiles.always.tokens`: standing cost per session

Also check stats if the ledger is available:

```bash
# tolkin-schema: stats --json
npx tolkin-cli@latest stats --json
```

Key fields from `tolkin stats --json`:

<!-- tolkin-schema: stats --json -->
```json
{
  "scope": "project",
  "project_key": "<str | null>",
  "generated_at": <unix_epoch_secs>,
  "prices_observed": "<str>",
  "realized_rate": {
    "usd_per_mtok_input": <f64>,
    "model": "<str>"
  },
  "ledger": {
    "records": <u64>,
    "skipped_lines": <u64>
  },
  "ingestion": {
    "enabled": <bool>,
    "sessions_scanned": <u64 | null>,
    "skipped_lines": <u64 | null>,
    "skipped_files": <u64 | null>
  },
  "tiers": {
    "identified": {
      "label": "advisory estimate",
      "project_reclaimable_min": <u64 | null>,
      "project_reclaimable_max": <u64 | null>,
      "project_as_of": <unix_epoch_secs | null>,
      "mcp_swap_tokens": <u64 | null>,
      "mcp_slim_tokens": <u64 | null>,
      "mcp_as_of": <unix_epoch_secs | null>,
      "audit_savings_min": <u64 | null>,
      "audit_savings_max": <u64 | null>,
      "audit_as_of": <unix_epoch_secs | null>,
      "projects": <u64>
    } | null,
    "realized": {
      "label": "measured structure, estimated frequency",
      "tokens": <i64>,
      "usd": <f64>,
      "sessions_basis": "measured" | "assumed",
      "sessions_count": <u64>,
      "since": <unix_epoch_secs>,
      "baseline_always_tokens": <u64>,
      "current_always_tokens": <u64>,
      "projects": <u64>
    } | null,
    "measured": {
      "label": "ground truth",
      "sessions": <u64>,
      "first_ts": <unix_epoch_secs | null>,
      "last_ts": <unix_epoch_secs | null>,
      "totals": {
        "input_tokens": <u64>,
        "output_tokens": <u64>,
        "cache_read_tokens": <u64>,
        "cache_write_5m_tokens": <u64>,
        "cache_write_1h_tokens": <u64>
      },
      "by_model": { "<model_id>": { "totals": { ... }, "cost_usd": <f64 | null> } },
      "cost_usd_total": <f64>,
      "unpriced_models": [{ "model": "<str>", "tokens": <u64> }],
      "cache_hit_rate": <f64>
    } | null,
    "notes": ["<str>"]
  }
}
```

All tiers are nested under `tiers`. If `tiers.realized` is non-null, report it:
"Realized savings (Tier 2) so far: ~N tokens removed from always-loaded context,
across ~M sessions (input-token estimate; output may vary)." The `tokens` field
is signed: negative means context grew since the baseline snapshot.

## Step 2: Triage findings

Group findings into three buckets:

**Safe (apply without lossy risk):**
- Minify embedded pretty-printed JSON (lossless; same semantic content).
- Deduplicate repeated paragraphs across files (consolidate, do not delete unique content).
- Move on-demand reference files out of always-loaded context into separate files
  (structural refactor; no content lost).
- Remove genuinely empty or placeholder sections.

**Review required (show to user first):**
- Splitting a large CLAUDE.md into per-subdirectory files (structural change with
  directory implications).
- Removing example blocks or code snippets (user must confirm they are not needed).
- Shortening a skill body (user must confirm the trimmed content is redundant).

**Opt-in only (do not apply unless explicitly requested):**
- Lossy compression or semantic rewriting of any kind.
- Removing factual context that might reduce output quality.
- Any change the user has not reviewed and approved.

## Step 3: Apply safe fixes (with confirmation per file)

For each safe fix:
1. Show the proposed change (file path, before/after token estimate, what changes).
2. Ask for confirmation.
3. Apply only after explicit approval.

Never batch-apply changes across multiple files without per-file confirmation.

## Step 4: Re-measure

After applying all approved fixes:

```bash
npx tolkin-cli@latest project . --json
```

Compare new `totals.context_tokens` against the baseline. This is the **realized
savings (Tier 2)** for this session.

If MCP configs were also modified via `tolkin mcp`, re-run that command too and
include its delta.

## Step 5: Summary report

Produce a summary structured as follows:

```
Tolkin optimization summary

Baseline:        <baseline_context_tokens> tokens in always-loaded context
After this pass: <new_context_tokens> tokens
Realized (Tier 2): ~<delta> tokens removed (input-token estimate; output may vary)

Identified remaining (Tier 1): <new_savings_min>-<new_savings_max> tokens still
reclaimable (advisory estimate; requires further edits).

Changes applied:
  - <file>: <description> (~<savings> tokens)
  ...

Changes skipped (require opt-in or were declined):
  - <description>

Next steps:
  - Run tolkin-slim to address MCP cold-load costs (if not already done).
  - Re-run this skill after any significant CLAUDE.md or skill edits.
  - tolkin stats --json shows realized savings accumulate over sessions once
    the ledger records enough snapshots.
```

## Tier vocabulary (always use these labels)

- **Identified (Tier 1)**: what the audit flags as reclaimable right now. Advisory.
  Not yet proven until the change is made and re-measured.
- **Realized (Tier 2)**: the measured delta between before and after in the same
  session. Structural evidence; session frequency estimated or user-supplied.
- **Measured (Tier 3)**: actual spend from ingested agent session logs. Ground truth.
  Only available when `tolkin stats` shows ingested usage data.

Never conflate tiers. Never quote a Tier 1 number as if it were Tier 3. All numbers
are input-token estimates; output tokens are not reduced by context slimming.
