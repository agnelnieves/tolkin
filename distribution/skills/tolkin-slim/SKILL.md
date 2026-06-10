---
name: tolkin-slim
description: |
  Tolkin MCP slim skill. Use when the user wants to reduce MCP server token costs,
  apply slim-profile snippets to their MCP config, swap a heavy MCP server for its
  CLI equivalent, or verify the delta after slimming. Also triggers on: "slim my MCP",
  "reduce MCP tokens", "tolkin mcp", "apply slim snippets", "MCP tool-definition cost".
metadata:
  version: 0.7.0
---

# tolkin-slim

Analyze an MCP config for tool-definition token cost, apply the emitted slim snippets
with the user's confirmation, re-run to verify the delta, and report realized savings
honestly with tier labels.

## When this skill applies

Use this skill when the user wants to reduce what MCP servers cost at context load time
(cold tokens), either by slimming the tool list or by swapping a server for a CLI.

## Step 1: Discover MCP configs (optional, if path unknown)

```bash
npx tolkin-cli@latest scan --json
# bunx works identically
```

Key fields from `tolkin scan --json`:

```
{
  "mcp": [
    {
      "client": "<str>",
      "path": "<str>",
      "analysis": { ... },
      "swaps": [
        {
          "server": "<str>",
          "cli": "<str>",
          "binary": "<str>",
          "installed": <bool>,
          "install_hint": "<str>",
          "savings_tokens": <u64>
        }
      ]
    }
  ],
  "totals": {
    "mcp_configs": <u64>,
    "mcp_cold_tokens": <u64>,
    "reclaimable_tokens": <u64>,
    "reclaimable_usd": <f64>,
    "instruction_files": <u64>,
    "instruction_tokens": <u64>,
    "shell_secret_count": <u64>,
    "provider": "<str>"
  }
}
```

If the config path is already known, skip directly to Step 2.

## Step 2: Analyze a specific MCP config

```bash
npx tolkin-cli@latest mcp <path-to-config> --json
```

Key fields from `tolkin mcp <config> --json` (shape: `McpAnalysis`):

```
{
  "client": "<str>",
  "provider": "<str>",
  "servers": [
    {
      "name": "<str>",
      "recommendation": "replace" | "replace*" | "keep" | "unknown",
      "tools": <u64 | null>,
      "savings_tokens": <u64>,
      "cli_alternative": "<str | null>",
      "note": "<str>",
      "scenarios": {
        "cold": <u64>,
        "warm": <u64>
      } | null,
      "slim": {
        "already_slimmed": <bool>,
        "est_tokens_saved": <u64>,
        "option": {
          "mechanism": "<str>",
          "snippet": "<str>"
        }
      } | null
    }
  ],
  "totals": {
    "servers": <u64>,
    "cold": <u64>,
    "warm": <u64>,
    "tool_search": <u64>,
    "pct_of_window": <u64>,
    "savings_tokens": <u64>,
    "savings_usd": <f64>,
    "slim_savings_tokens": <u64>,
    "slim_savings_usd": <f64>,
    "unknown": <u64>
  },
  "notes": ["<str>"]
}
```

## Step 3: Interpret findings

Two types of savings are independent; do not add them together:

- **Swap savings** (`totals.savings_tokens`): replace an MCP server entirely with
  its CLI equivalent. Use when the user rarely needs the interactive server flow.
  `recommendation: "replace"` means swap is recommended. `"replace*"` means replace
  for ad-hoc use but keep for specific flows noted in `note`.
- **Slim savings** (`totals.slim_savings_tokens`): keep the server but register fewer
  tools using the mechanism in `servers[n].slim.option.snippet`. Use when the user
  needs the server but not all its tools loaded cold.

For each server with a non-null `slim` and `slim.already_slimmed == false`:
- Show the `slim.option.snippet` to the user (this is copy-pasteable config).
- State the estimated saving as `slim.est_tokens_saved` tokens (identified, Tier 1).

## Step 4: Apply snippets (with user confirmation)

Show each snippet and ask for confirmation before applying. Do not edit MCP config
files silently.

When the user confirms, apply the snippet to the appropriate server entry in the config
file. The snippet is the exact JSON or JSONC fragment to add or replace.

## Step 5: Verify the delta

After applying, re-run:

```bash
npx tolkin-cli@latest mcp <path-to-config> --json
```

Compare `totals.cold` before and after. The delta is the **realized savings (Tier 2)**
for this config. Report it as:

"Realized savings (Tier 2): ~N tokens removed from MCP cold-load for <client>
(input-token estimate; output may vary). Identified savings were ~M tokens; realized
N tokens."

If the before/after delta is less than the identified estimate, note the discrepancy
honestly. Identified savings are advisory; realized savings are what the re-run shows.

## Honesty rules

- Swap and slim savings are alternatives for the same server, not additive.
- Never promise dollar savings in the chat; point at the `savings_usd` field and note
  it uses observed pricing that may be stale (`PRICES_OBSERVED` label in the output).
- All savings figures are input-token estimates. Output tokens are not affected by
  MCP cold-load reduction.
- `already_slimmed: true` means the server is already using the filtering mechanism;
  report this as "already slimmed" and move on.
