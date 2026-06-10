"use client";

import { useEffect, useRef, useState } from "react";
import type {
  CoreProvider,
  McpAnalysis,
  McpServerReport,
  McpSlimRecommendation,
  Recommendation,
} from "../lib/core";
import { analyzeMcp } from "../lib/core";

// MCP config analyzer. Paste any AI agent's MCP config (Claude Desktop, Claude
// Code, Cursor, Continue, VS Code / Copilot, Zed) and see the token cost of its
// tool definitions plus which servers to swap for an official CLI. This is a
// self-contained client component with its own state, separate from the
// tokenizer analyzer. All parsing, the server catalog, and the cost math live in
// the WASM core via analyzeMcp; nothing here recomputes any of it. The pasted
// config never leaves the browser and is only ever rendered back in the textarea
// the user typed into. The analysis output carries server names and estimates
// only, never env or secret values.

// A small realistic config so the panel renders a populated analysis on first
// paint. It is the default textarea content, not committed user data.
const SAMPLE_CONFIG = `{
  "mcpServers": {
    "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] },
    "linear": { "url": "https://mcp.linear.app/sse", "type": "sse" },
    "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "~"] },
    "notion": { "command": "npx", "args": ["@notionhq/notion-mcp-server"] }
  }
}`;

const PROVIDER_LABELS: Record<CoreProvider, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  gemini: "Gemini",
};

type AnalysisState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ok"; analysis: McpAnalysis }
  | { status: "error"; message: string };

export function McpAnalyzer() {
  const [config, setConfig] = useState(SAMPLE_CONFIG);
  const [provider, setProvider] = useState<CoreProvider>("anthropic");
  const [state, setState] = useState<AnalysisState>({ status: "idle" });
  const runRef = useRef(0);

  // Debounced analysis. Mirrors cost-panel: a short debounce plus a run-id guard
  // so a slow earlier run cannot overwrite a newer result. An empty config skips
  // the core call and shows the friendly empty state.
  useEffect(() => {
    if (config.trim() === "") {
      runRef.current++;
      setState({ status: "idle" });
      return;
    }
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setState({ status: "loading" });
      analyzeMcp(config, provider).then(
        (analysis) => {
          if (runRef.current !== runId) return;
          setState({ status: "ok", analysis });
        },
        (e: unknown) => {
          if (runRef.current !== runId) return;
          setState({ status: "error", message: errorMessage(e) });
        },
      );
    }, 150);
    return () => clearTimeout(handle);
  }, [config, provider]);

  return (
    <section className="w-full space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-sm font-medium text-zinc-300">MCP analyzer</h2>
          <p className="text-xs text-zinc-500">
            Paste any agent's MCP config. See what its tool definitions cost and which servers to
            swap for a CLI.
          </p>
        </div>
        <label className="flex items-center gap-2 text-xs text-zinc-400">
          <span>Provider</span>
          <select
            value={provider}
            onChange={(e) => setProvider(e.target.value as CoreProvider)}
            className="rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 focus:border-zinc-600 focus:outline-none"
          >
            <option value="anthropic">Anthropic</option>
            <option value="openai">OpenAI</option>
            <option value="gemini">Gemini</option>
          </select>
        </label>
      </div>

      <label className="block">
        <span className="sr-only">Paste an MCP config to analyze</span>
        <textarea
          value={config}
          onChange={(e) => setConfig(e.target.value)}
          placeholder="Paste an MCP config. Nothing leaves your browser."
          rows={10}
          spellCheck={false}
          className="w-full resize-y rounded-lg border border-zinc-800 bg-zinc-950 px-4 py-3 font-mono text-sm leading-6 text-zinc-100 placeholder:text-zinc-600 focus:border-zinc-600 focus:outline-none focus:ring-2 focus:ring-zinc-700"
        />
      </label>

      {state.status === "error" ? (
        <ErrorBox message={state.message} />
      ) : state.status === "idle" ? (
        <EmptyState />
      ) : state.status === "loading" ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
          analyzing...
        </p>
      ) : (
        <Results analysis={state.analysis} provider={provider} />
      )}

      <p className="border-t border-zinc-800 pt-3 text-xs leading-5 text-zinc-500">
        Estimates are input-token-bounded. Your config never leaves the browser.
      </p>
    </section>
  );
}

function Results({ analysis, provider }: { analysis: McpAnalysis; provider: CoreProvider }) {
  const { totals, servers, client, notes } = analysis;
  const swappable = servers.filter(
    (s) => s.recommendation === "replace" || s.recommendation === "replace-for-ad-hoc",
  ).length;
  const slimmable = servers.filter(
    (s) => s.slim != null && !s.slim.already_slimmed && s.slim.est_tokens_saved > 0,
  ).length;

  return (
    <div className="space-y-4">
      <Headline
        savingsTokens={totals.savings_tokens}
        savingsUsd={totals.savings_usd}
        swappable={swappable}
        slimTokens={totals.slim_savings_tokens}
        slimmable={slimmable}
        provider={provider}
      />

      <div className="space-y-2 rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <span className="text-xs text-zinc-400">
            Detected <span className="font-medium text-zinc-200">{client}</span>
          </span>
          <span className="text-[11px] tabular-nums text-zinc-500">
            {totals.servers} {totals.servers === 1 ? "server" : "servers"}, {totals.matched} matched
            {totals.unknown > 0 ? `, ${totals.unknown} unknown` : ""}
          </span>
        </div>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <TotalStat label="cold" value={totals.cold} />
          <TotalStat label="warm" value={totals.warm} />
          <TotalStat label="tool search" value={totals.tool_search} />
          <TotalStat label="% of 200K" value={`${formatPct(totals.pct_of_window)}%`} raw />
        </div>
      </div>

      <ServerTable servers={servers} />

      {notes.length > 0 ? (
        <div className="space-y-1 text-xs leading-5 text-zinc-500">
          {notes.map((n) => (
            <p key={n}>{n}</p>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function Headline({
  savingsTokens,
  savingsUsd,
  swappable,
  slimTokens,
  slimmable,
  provider,
}: {
  savingsTokens: number;
  savingsUsd: number;
  swappable: number;
  slimTokens: number;
  slimmable: number;
  provider: CoreProvider;
}) {
  const hasSavings = savingsTokens > 0 && swappable > 0;
  const hasSlim = slimTokens > 0 && slimmable > 0;

  if (!hasSavings && !hasSlim) {
    return (
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
        <p className="text-sm text-zinc-300">No CLI swaps or slim options found for this config.</p>
        <p className="mt-1 text-xs text-zinc-500">
          The servers here are already lean or are not in the catalog. Nothing to reclaim.
        </p>
      </div>
    );
  }

  if (!hasSavings) {
    return (
      <div className="rounded-lg border border-emerald-900/60 bg-emerald-950/30 p-4">
        <p className="text-[10px] uppercase tracking-wider text-emerald-400/80">
          reclaimable per cold session (estimate)
        </p>
        <p className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <span className="text-3xl font-semibold tabular-nums text-emerald-200">
            ~{slimTokens.toLocaleString()}
          </span>
          <span className="text-sm text-emerald-300/80">tokens by slimming</span>
        </p>
        <p className="mt-2 text-xs text-emerald-300/70">
          Keep {slimmable === 1 ? "this server" : `these ${slimmable} servers`} but register fewer
          tools. Snippets are in the table below.
        </p>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-emerald-900/60 bg-emerald-950/30 p-4">
      <p className="text-[10px] uppercase tracking-wider text-emerald-400/80">
        reclaimable per cold session
      </p>
      <p className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="text-3xl font-semibold tabular-nums text-emerald-200">
          {savingsTokens.toLocaleString()}
        </span>
        <span className="text-sm text-emerald-300/80">tokens</span>
        <span className="text-2xl font-semibold tabular-nums text-emerald-200">
          {formatUsd(savingsUsd)}
        </span>
        <span className="text-sm text-emerald-300/80">at {PROVIDER_LABELS[provider]} rates</span>
      </p>
      <p className="mt-2 text-xs text-emerald-300/70">
        Swap {swappable} {swappable === 1 ? "server" : "servers"} for an official CLI to reclaim
        this context.
      </p>
      {hasSlim ? (
        <p className="mt-1 text-xs text-emerald-300/70">
          Or keep {slimmable === 1 ? "a server" : "servers"} and slim{" "}
          {slimmable === 1 ? "it" : "them"}: ~{slimTokens.toLocaleString()} tokens by registering
          fewer tools. Per server, swap and slim are alternatives, not additive.
        </p>
      ) : null}
    </div>
  );
}

function ServerTable({ servers }: { servers: McpServerReport[] }) {
  if (servers.length === 0) {
    return (
      <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
        No servers found in this config.
      </p>
    );
  }

  return (
    <div className="overflow-hidden rounded-lg border border-zinc-800">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-zinc-800 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500">
            <th className="px-3 py-2 font-medium">Server</th>
            <th className="px-3 py-2 text-right font-medium">Tools</th>
            <th className="px-3 py-2 text-right font-medium">Cold tok</th>
            <th className="px-3 py-2 font-medium">CLI swap</th>
            <th className="px-3 py-2 text-right font-medium">Savings</th>
          </tr>
        </thead>
        <tbody>
          {servers.map((s) => (
            <ServerRow key={s.name} server={s} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ServerRow({ server }: { server: McpServerReport }) {
  const [slimOpen, setSlimOpen] = useState(false);
  const slim = server.slim;

  return (
    <>
      <tr className="border-b border-zinc-900 last:border-b-0">
        <td className="px-3 py-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-zinc-200">{server.display || server.name}</span>
            <RecommendationBadge recommendation={server.recommendation} />
            {slim != null ? (
              slim.already_slimmed ? (
                <span className="rounded bg-zinc-800/80 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-500">
                  slimmed
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => setSlimOpen((open) => !open)}
                  aria-expanded={slimOpen}
                  className="rounded bg-sky-950 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-sky-300 hover:bg-sky-900 focus:outline-none focus:ring-1 focus:ring-sky-600"
                >
                  slim available {slimOpen ? "▾" : "▸"}
                </button>
              )
            ) : null}
          </div>
        </td>
        <td className="px-3 py-2 text-right tabular-nums text-zinc-400">{server.tools ?? "-"}</td>
        <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
          {server.cold_tokens != null ? server.cold_tokens.toLocaleString() : "-"}
        </td>
        <td className="px-3 py-2 font-mono text-zinc-300">{server.cli_alternative ?? "-"}</td>
        <td className="px-3 py-2 text-right tabular-nums">
          {server.savings_tokens > 0 ? (
            <span className="text-emerald-300">{server.savings_tokens.toLocaleString()}</span>
          ) : (
            <span className="text-zinc-600">-</span>
          )}
        </td>
      </tr>
      {slim != null && !slim.already_slimmed && slimOpen ? (
        <tr className="border-b border-zinc-900 last:border-b-0">
          <td colSpan={5} className="px-3 pb-3">
            <SlimDetails slim={slim} />
          </td>
        </tr>
      ) : null}
      {/* Catalog notes render for every matched server (keep servers carry the
          no-native-filtering guidance); unknown servers keep their generic note
          out of the table. */}
      {server.note && server.matched_id != null ? (
        <tr className="border-b border-zinc-900 last:border-b-0">
          <td colSpan={5} className="px-3 pb-2 text-[11px] leading-5 text-zinc-500">
            {server.note}
          </td>
        </tr>
      ) : null}
    </>
  );
}

function SlimDetails({ slim }: { slim: McpSlimRecommendation }) {
  return (
    <div className="space-y-2 rounded-md border border-sky-900/50 bg-sky-950/20 p-3">
      <p className="text-[11px] leading-5 text-sky-200">
        <span className="font-medium">{slim.option.mechanism}</span>
        {slim.est_tokens_saved > 0 ? (
          <span className="text-sky-300/80">
            {" "}
            saves an estimated ~{slim.est_tokens_saved.toLocaleString()} input tokens if you keep
            this server.
          </span>
        ) : null}
      </p>
      <div className="flex items-start gap-2">
        <pre className="min-w-0 flex-1 overflow-x-auto rounded bg-zinc-950 px-3 py-2 font-mono text-[11px] leading-5 text-zinc-200">
          {slim.option.snippet}
        </pre>
        <CopyButton text={slim.option.snippet} />
      </div>
      <p className="text-[11px] leading-5 text-zinc-500">{slim.option.note}</p>
      <p className="text-[10px] text-zinc-600">{slim.option.source_hint}</p>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        navigator.clipboard
          .writeText(text)
          .then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          })
          .catch(() => {
            // Clipboard access denied; the snippet stays selectable by hand.
          });
      }}
      className="shrink-0 rounded border border-zinc-700 px-2 py-1 text-[10px] uppercase tracking-wider text-zinc-400 hover:border-zinc-500 hover:text-zinc-200 focus:outline-none focus:ring-1 focus:ring-zinc-500"
    >
      {copied ? "copied" : "copy"}
    </button>
  );
}

function RecommendationBadge({ recommendation }: { recommendation: Recommendation }) {
  const styles: Record<Recommendation, string> = {
    replace: "bg-emerald-950 text-emerald-300",
    "replace-for-ad-hoc": "bg-amber-950 text-amber-300",
    keep: "bg-blue-950 text-blue-300",
    unknown: "bg-zinc-800 text-zinc-400",
  };
  const labels: Record<Recommendation, string> = {
    replace: "replace",
    "replace-for-ad-hoc": "ad-hoc",
    keep: "keep",
    unknown: "unknown",
  };
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider ${styles[recommendation]}`}
    >
      {labels[recommendation]}
    </span>
  );
}

function TotalStat({
  label,
  value,
  raw,
}: {
  label: string;
  value: number | string;
  raw?: boolean;
}) {
  const display = raw ? value : typeof value === "number" ? value.toLocaleString() : value;
  return (
    <div className="space-y-1">
      <span className="block text-[10px] uppercase tracking-wider text-zinc-500">{label}</span>
      <span className="block font-mono text-base tabular-nums text-zinc-200">{display}</span>
    </div>
  );
}

function EmptyState() {
  return (
    <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
      Paste an MCP config above to see its token cost and swap recommendations.
    </p>
  );
}

function ErrorBox({ message }: { message: string }) {
  return (
    <div className="space-y-1 rounded-lg border border-amber-900/60 bg-amber-950/20 p-4">
      <p className="text-xs font-medium text-amber-300">Could not parse this config.</p>
      <p className="font-mono text-[11px] leading-5 text-amber-300/70">{message}</p>
    </div>
  );
}

// Format a dollar amount with enough precision that sub-cent figures stay
// legible. Mirrors cost-panel.tsx: large values round to cents, small ones keep
// up to four decimals.
function formatUsd(value: number): string {
  if (value === 0) return "$0";
  if (value >= 1) return `$${value.toFixed(2)}`;
  if (value >= 0.01) return `$${value.toFixed(3)}`;
  return `$${value.toFixed(4)}`;
}

// Percentage of the context window. Keeps one decimal under 10%, whole numbers
// above, so a 0.4% server does not collapse to 0%.
function formatPct(value: number): string {
  if (value >= 10) return Math.round(value).toString();
  return value.toFixed(1);
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
