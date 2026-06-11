---
name: tolkin-cache
description: |
  Tolkin prompt-cache health skill. Use when the user asks about cache hit rate,
  cache write churn, TTL cost tradeoffs, prefix stability, or why caching feels
  broken. Also triggers on: "is my cache working", "cache hit rate", "why are
  cache writes expensive", "5m vs 1h TTL", "prompt cache health", "broken cache
  advisory", "tolkin cache", "session shape and caching".
metadata:
  version: 0.11.0
---

# tolkin-cache

Measure prompt-cache health from ingested session logs, interpret the findings
with correct tier labels, and identify prefix-stability fixes. Never touch
runtime cache_control; the levers here are structural (what sits in the first
KB of context).

## When this skill applies

Use this skill when the user wants to understand whether their prompt cache is
working, whether the 5-minute or 1-hour TTL is cheaper for their session
cadence, or why cache hit rate is low. Requires `tolkin stats` log ingestion to
be on; the skill explains what to do when it is off.

## Step 1: Run the cache health report

```bash
npx tolkin-cli@latest cache --json
# bunx works identically; add --global for machine-wide scope
# npx tolkin-cli@latest cache --global --json
```

The command reads ingested Claude Code session logs from the local ledger and
computes cache health facts from the per-request tuples retained there.

Key fields from `tolkin cache --json` (project scope):

<!-- tolkin-schema: cache --json -->
```json
{
  "scope": "project",
  "project_key": null,
  "generated_at": null,
  "prices_observed": null,
  "cache": {
    "source": null,
    "sessions_analyzed": null,
    "requests_analyzed": null,
    "hit_rate": {
      "label": null,
      "rate": null,
      "cache_read_tokens": null,
      "input_side_tokens": null,
      "threshold": null,
      "days_below_threshold": [],
      "advisory": null
    },
    "write_churn": {
      "label": null,
      "total_write_tokens": null,
      "writes_after_first_tokens": null,
      "share": null,
      "sessions_with_writes": null,
      "worst_sessions": []
    },
    "ttl_counterfactual": {
      "label": null,
      "inputs_label": null,
      "tier_note": null,
      "observed_write_tokens_5m": null,
      "observed_write_tokens_1h": null,
      "sessions_simulated": null,
      "sessions_without_writes_excluded": null,
      "simulated_w5_write_events": null,
      "simulated_w1_write_events": null,
      "simulated_w5_write_tokens": null,
      "simulated_w1_write_tokens": null,
      "prefix_proxy": null,
      "marginal_multiplier_5m": null,
      "marginal_multiplier_1h": null,
      "usd_5m_strategy": null,
      "usd_1h_strategy": null,
      "usd_delta_1h_minus_5m": null,
      "priced_share_of_simulated_tokens": null,
      "unpriced_models": [],
      "break_even": null,
      "verdict": null
    },
    "cadence": {
      "label": null,
      "intra_session_gaps": null,
      "intra_session_gaps_over_5m": null,
      "share_intra_gaps_over_5m": null,
      "inter_session_gaps": null,
      "inter_session_gaps_under_1h": null,
      "share_inter_gaps_under_1h": null,
      "sessions_zero_cache_read": null
    },
    "scope_line": null,
    "notes": []
  }
}
```

Key fields from `tolkin cache --global --json` (machine-wide scope):

<!-- tolkin-schema: cache --global --json -->
```json
{
  "scope": "global",
  "project_key": null,
  "generated_at": null,
  "prices_observed": null,
  "cache": {
    "source": null,
    "sessions_analyzed": null,
    "requests_analyzed": null,
    "hit_rate": {
      "label": null,
      "rate": null,
      "cache_read_tokens": null,
      "input_side_tokens": null,
      "threshold": null,
      "days_below_threshold": [],
      "advisory": null
    },
    "write_churn": {
      "label": null,
      "total_write_tokens": null,
      "writes_after_first_tokens": null,
      "share": null,
      "sessions_with_writes": null,
      "worst_sessions": []
    },
    "ttl_counterfactual": {
      "label": null,
      "inputs_label": null,
      "tier_note": null,
      "observed_write_tokens_5m": null,
      "observed_write_tokens_1h": null,
      "sessions_simulated": null,
      "sessions_without_writes_excluded": null,
      "simulated_w5_write_events": null,
      "simulated_w1_write_events": null,
      "simulated_w5_write_tokens": null,
      "simulated_w1_write_tokens": null,
      "prefix_proxy": null,
      "marginal_multiplier_5m": null,
      "marginal_multiplier_1h": null,
      "usd_5m_strategy": null,
      "usd_1h_strategy": null,
      "usd_delta_1h_minus_5m": null,
      "priced_share_of_simulated_tokens": null,
      "unpriced_models": [],
      "break_even": null,
      "verdict": null
    },
    "cadence": {
      "label": null,
      "intra_session_gaps": null,
      "intra_session_gaps_over_5m": null,
      "share_intra_gaps_over_5m": null,
      "inter_session_gaps": null,
      "inter_session_gaps_under_1h": null,
      "share_inter_gaps_under_1h": null,
      "sessions_zero_cache_read": null
    },
    "scope_line": null,
    "notes": []
  }
}
```

When `cache` is null, ingestion is off or the ledger is not yet initialized;
the `hints` array (present only in that case) explains how to enable it.

**Scope note:** use `--json` for a single project's health; use `--global --json`
for a machine-wide view across all projects. The `scope_line` field in the
response states what the actionable levers are for Claude Code users.

## Step 2: Read the output with correct tier labels

Each section carries a `label` that names its tier. Use these labels verbatim
when reporting numbers:

### hit_rate (ground truth, Tier 3)

`hit_rate.rate` is cache reads divided by all input-side tokens over the scope.
A rate above 0.5 on every active day is normal for an agent with a stable
prefix; the broken-cache threshold is 0.5.

`hit_rate.advisory` is non-null when at least one active day fell under the
threshold. Report it as: "cache hit rate fell under 50% on N active day(s)
(ground truth)." Name the days from `days_below_threshold`.

`hit_rate.sessions_analyzed` and `requests_analyzed` are the sample size.
Fewer than 10 sessions means the numbers are thin; note it.

### write_churn (ground truth, Tier 3)

`write_churn.share` is cache writes after a session's first write divided by
total write tokens. A high share on Claude Code transcripts is expected: every
appended turn writes a new suffix by construction. The `scope_line` and
`notes[1]` in the response state this explicitly. Report churn comparatively
("session X has unusually high churn compared to the others") rather than as a
defect score on its own.

### ttl_counterfactual (advisory estimate, Tier 1, computed from Tier 3 inputs)

Every number in `ttl_counterfactual` is a Tier 1 advisory estimate. The inputs
(`hit_rate`, `write_churn`, gap data) are ground truth; the simulated strategies
never ran, so their outputs are advisory. Always say "advisory estimate" when
quoting these numbers.

The verdict is in `ttl_counterfactual.verdict`. Quote it as an advisory
estimate, not a directive. The `tier_note` field carries the full disclosure
statement.

Key values:
- `simulated_w5_write_tokens` / `simulated_w1_write_tokens`: what each strategy
  would cost in write tokens if the prefix were stable.
- `usd_5m_strategy` / `usd_1h_strategy`: dollar cost at per-model rates. Priced
  models only; `priced_share_of_simulated_tokens` and `unpriced_models` disclose
  the coverage.
- `usd_delta_1h_minus_5m`: positive means 1h is more expensive; negative means
  1h is cheaper. Quote as advisory.

The `break_even` field states the exact condition under which the 1h TTL wins.
Availability footnote: the 1h TTL is not available on every model and platform;
`notes[3]` in the response carries the current availability note with a
reference URL.

### cadence (ground truth, Tier 3)

`cadence` records measured timeline facts:
- `share_inter_gaps_under_1h`: how often consecutive sessions of the same
  project were under one hour apart; high values mean a 1h cache could survive
  between sessions.
- `share_intra_gaps_over_5m`: how often gaps inside a session exceeded 5
  minutes; high values mean the 5m TTL would need a re-write mid-session.
- `sessions_zero_cache_read`: sessions where caching never engaged; if this
  is high relative to `sessions_analyzed`, caching may not be engaging at all.

### The scope_line

The `scope_line` field is the primary actionable summary for Claude Code users.
Quote it when explaining what to do: Claude Code manages its own caching, so the
levers are prefix stability and session shape, not TTL choice. TTL choice is
only a direct lever for API pipeline builders.

## Step 3: Apply prefix-stability fixes ONLY with explicit user confirmation

When `hit_rate.advisory` is non-null (broken-cache advisory), or when
`write_churn` shows unusually volatile sessions, the actionable fix is
improving prefix stability. This means:

1. Move volatile content (timestamps, dynamic summaries, per-run stats) out of
   the first 1,024 tokens of always-loaded context. Prompt caching checkpoints
   at the cache threshold boundary; content before that boundary must not change
   between requests for the cache to survive.
2. Move stable context toward the front: instruction files, tool definitions, and
   skill bodies that do not change between sessions are good candidates.
3. Consolidate frequently-changing blocks toward the end of the context, after
   stable anchors.

To find specific volatile-prefix findings, run:

```bash
npx tolkin-cli@latest audit <file> --json
```

Look for findings with `rule: "volatile-prefix"` in the output.

**NEVER apply these fixes without explicit user confirmation.** Show the proposed
change (which file, which section, what moves) and wait for approval before
editing. Do not touch `cache_control` annotations in code; those are runtime
settings and outside this skill's scope.

## Step 4: Re-run and report the delta with tier labels

After the user applies prefix-stability edits:

```bash
npx tolkin-cli@latest cache --json
```

Compare `hit_rate.rate` and `write_churn.share` before and after. These are
ground truth (Tier 3) when the numbers come from ingested session logs after
the fix has been running for at least a few sessions.

Report the delta as: "Cache hit rate moved from X% to Y% (ground truth, Tier 3,
over N sessions). Input-token estimate; output may vary."

Do not quote the before-and-after TTL counterfactual numbers as realized savings;
they remain advisory estimates because the simulated strategy still never ran.
The realized improvement is the change in the ground-truth hit rate.

## Honesty rules

- `hit_rate`, `write_churn`, and `cadence` are all ground truth (Tier 3,
  measured). Report them as facts from the ingested logs.
- Every number in `ttl_counterfactual` is an advisory estimate (Tier 1),
  computed from ground-truth inputs. Always say "advisory estimate" when
  quoting them.
- Counterfactuals are not savings. A lower `usd_1h_strategy` does not mean
  money was saved; it means that strategy would have cost less on the observed
  gap timeline, had it run.
- `cache` being null means no ingested data, not a zero hit rate. Explain the
  difference.
- The 1h TTL requires per-model support; quote the availability footnote from
  `notes[3]` when recommending it.
- All token estimates are input-side bounded. Output tokens are not reduced by
  prefix-stability improvements.
