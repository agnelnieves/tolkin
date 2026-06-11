"use client";

import { useEffect, useRef, useState } from "react";
import { EmptyHint, PanelCard, PanelHeader } from "@/components/analyzer/panel";
import type {
  CoreProvider,
  McpAnalysis,
  McpServerReport,
  McpSlimRecommendation,
  McpToolInventory,
  McpToolTokenCount,
  Recommendation,
} from "../lib/core";
import { analyzeMcp, analyzeToolsInventory, parseToolsList } from "../lib/core";
import { count as countTokens } from "../lib/tokenize";

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
    <PanelCard>
      <PanelHeader
        title="MCP analyzer"
        blurb="Paste any agent's MCP config. See what its tool definitions cost and which servers to swap for a CLI."
        action={
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="font-mono uppercase tracking-[0.16em]">Provider</span>
            <select
              value={provider}
              onChange={(e) => setProvider(e.target.value as CoreProvider)}
              className="rounded-md border border-white/10 bg-black/40 px-2 py-1.5 text-xs text-foreground focus:border-lime-300/60 focus:outline-none"
            >
              <option value="anthropic">Anthropic</option>
              <option value="openai">OpenAI</option>
              <option value="gemini">Gemini</option>
            </select>
          </label>
        }
      />

      <label className="block">
        <span className="sr-only">Paste an MCP config to analyze</span>
        <textarea
          value={config}
          onChange={(e) => setConfig(e.target.value)}
          placeholder="Paste an MCP config. Nothing leaves your browser."
          rows={10}
          spellCheck={false}
          className="block w-full resize-y rounded-lg border border-white/10 bg-black/40 px-4 py-3 font-mono text-base leading-6 text-foreground placeholder:text-muted-foreground/60 focus:border-lime-300/60 focus:outline-none focus:ring-2 focus:ring-lime-300/40 sm:text-sm"
        />
      </label>

      <div className="mt-4">
        {state.status === "error" ? (
          <ErrorBox message={state.message} />
        ) : state.status === "idle" ? (
          <EmptyHint>
            Paste an MCP config above to see its token cost and swap recommendations.
          </EmptyHint>
        ) : state.status === "loading" ? (
          <EmptyHint>analyzing</EmptyHint>
        ) : (
          <Results analysis={state.analysis} provider={provider} />
        )}
      </div>

      <ToolsListSection provider={provider} />

      <p className="mt-4 border-t border-white/10 pt-3 text-xs leading-5 text-muted-foreground">
        Estimates are input-token-bounded. Your config never leaves the browser.
      </p>
    </PanelCard>
  );
}

// ---- tools/list manifest analysis (exact per-tool counts) ----

// Paste a server's tools/list JSON for exact tokenized counts. The core
// parses and canonicalizes each tool ({name, description, input_schema},
// compact); the browser tokenizes every serialization and description with
// the panel's o200k_base tokenizer (gpt-tokenizer, in the tokenize worker);
// the core then builds the per-tool table and description smells. Counts are
// exact for the canonical serialization and labeled "tokenized manifest
// (o200k_base)"; they supersede catalog estimates, never blend with them.
function ToolsListSection({ provider }: { provider: CoreProvider }) {
  const [manifest, setManifest] = useState("");
  const [serverName, setServerName] = useState("");
  const [state, setState] = useState<AnalysisState>({ status: "idle" });
  const runRef = useRef(0);

  useEffect(() => {
    if (manifest.trim() === "") {
      runRef.current++;
      setState({ status: "idle" });
      return;
    }
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setState({ status: "loading" });
      void (async () => {
        try {
          const specs = await parseToolsList(manifest);
          const tools: McpToolTokenCount[] = await Promise.all(
            specs.map(async (spec) => {
              const [full, desc] = await Promise.all([
                countTokens("openai", spec.serialized),
                countTokens("openai", spec.description),
              ]);
              return {
                name: spec.name,
                description: spec.description,
                tokens: full.tokens,
                description_tokens: desc.tokens,
              };
            }),
          );
          const analysis = await analyzeToolsInventory(
            serverName.trim() === "" ? "server" : serverName.trim(),
            "o200k_base",
            tools,
            provider,
          );
          if (runRef.current !== runId) return;
          setState({ status: "ok", analysis });
        } catch (e: unknown) {
          if (runRef.current !== runId) return;
          setState({ status: "error", message: errorMessage(e) });
        }
      })();
    }, 300);
    return () => clearTimeout(handle);
  }, [manifest, serverName, provider]);

  const detail = state.status === "ok" ? (state.analysis.servers[0]?.tools_detail ?? null) : null;

  return (
    <div className="mt-5 space-y-3 border-t border-white/10 pt-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0 space-y-1.5">
          <h3 className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
            tools/list manifest (exact counts)
          </h3>
          <p className="max-w-prose text-[13px] leading-relaxed text-muted-foreground">
            Paste a server's tools/list JSON for exact per-tool counts and description smells.
            Tokenized with o200k_base in your browser; nothing leaves it.
          </p>
        </div>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono uppercase tracking-[0.16em]">Server name</span>
          <input
            type="text"
            value={serverName}
            onChange={(e) => setServerName(e.target.value)}
            placeholder="e.g. github"
            spellCheck={false}
            className="w-36 rounded-md border border-white/10 bg-black/40 px-2 py-1.5 font-mono text-xs text-foreground placeholder:text-muted-foreground/60 focus:border-lime-300/60 focus:outline-none"
          />
        </label>
      </div>

      <label className="block">
        <span className="sr-only">Paste a tools/list JSON manifest to analyze</span>
        <textarea
          value={manifest}
          onChange={(e) => setManifest(e.target.value)}
          placeholder={
            'Paste tools/list JSON: {"tools": [...]}, a bare tool array, or the full JSON-RPC response.'
          }
          rows={6}
          spellCheck={false}
          className="block w-full resize-y rounded-lg border border-white/10 bg-black/40 px-4 py-3 font-mono text-base leading-6 text-foreground placeholder:text-muted-foreground/60 focus:border-lime-300/60 focus:outline-none focus:ring-2 focus:ring-lime-300/40 sm:text-sm"
        />
      </label>

      {state.status === "error" ? (
        <ErrorBox message={state.message} />
      ) : state.status === "loading" ? (
        <EmptyHint>tokenizing manifest</EmptyHint>
      ) : state.status === "ok" ? (
        <div className="space-y-4">
          <Results analysis={state.analysis} provider={provider} />
          {detail != null ? <ToolsDetailView detail={detail} /> : null}
        </div>
      ) : null}
    </div>
  );
}

function ToolsDetailView({ detail }: { detail: McpToolInventory }) {
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h4 className="font-mono text-[11px] font-medium uppercase tracking-[0.2em] text-muted-foreground">
          Per-tool breakdown
        </h4>
        <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
          {detail.tool_count} tools, {detail.total_tokens.toLocaleString()} tokens,{" "}
          <span className="text-lime-300">{detail.basis}</span>
        </span>
      </div>

      <div className="max-h-96 overflow-auto rounded-lg border border-white/10">
        <table className="w-full border-collapse text-left text-xs">
          <thead className="sticky top-0 bg-black/80 backdrop-blur">
            <tr className="border-b border-white/10 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
              <th className="px-3 py-2 font-medium">Tool</th>
              <th className="px-3 py-2 text-right font-medium">Tokens</th>
              <th className="px-3 py-2 text-right font-medium">Share of server</th>
              <th className="px-3 py-2 text-right font-medium">Desc tokens</th>
            </tr>
          </thead>
          <tbody>
            {detail.tools.map((row) => (
              <tr key={row.name} className="border-b border-white/5 last:border-b-0">
                <td className="px-3 py-1.5 font-mono text-foreground">{row.name}</td>
                <td className="px-3 py-1.5 text-right font-mono tabular-nums text-muted-foreground">
                  {row.tokens.toLocaleString()}
                </td>
                <td className="px-3 py-1.5 text-right font-mono tabular-nums text-muted-foreground">
                  {row.share_pct.toFixed(2)}%
                </td>
                <td className="px-3 py-1.5 text-right font-mono tabular-nums text-muted-foreground/80">
                  {row.description_tokens.toLocaleString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {detail.smells.length === 0 ? (
        <EmptyHint>Description smells: none found.</EmptyHint>
      ) : (
        <div className="space-y-2">
          <h4 className="font-mono text-[11px] font-medium uppercase tracking-[0.2em] text-muted-foreground">
            Description smells ({detail.smells.length})
          </h4>
          {detail.smells.map((smell) => (
            <div
              key={`${smell.rule}:${smell.tools.join(",")}`}
              className="space-y-1 rounded-lg border border-amber-400/30 bg-amber-400/5 p-3"
            >
              <p className="text-xs">
                <span className="rounded border border-amber-400/30 bg-amber-400/10 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-[0.18em] text-amber-300">
                  {smell.rule}
                </span>{" "}
                <span className="font-mono text-[11px] text-foreground">
                  {smell.tools.length > 6
                    ? `${smell.tools.slice(0, 6).join(", ")} and ${smell.tools.length - 6} more`
                    : smell.tools.join(", ")}
                </span>
              </p>
              <p className="text-[11px] leading-5 text-muted-foreground">{smell.detail}</p>
              <p className="text-[11px] leading-5 text-amber-300/90">{smell.recommendation}</p>
            </div>
          ))}
        </div>
      )}

      <div className="space-y-1 text-[11px] leading-5 text-muted-foreground">
        {detail.notes.map((n) => (
          <p key={n}>{n}</p>
        ))}
      </div>
    </div>
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

      <div className="space-y-3 rounded-lg border border-white/10 bg-black/30 p-4">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <span className="text-xs text-muted-foreground">
            Detected <span className="font-medium text-foreground">{client}</span>
          </span>
          <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
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
        <div className="space-y-1 text-xs leading-5 text-muted-foreground">
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
      <div className="rounded-lg border border-white/10 bg-black/30 p-4">
        <p className="text-sm text-foreground">
          No CLI swaps or slim options found for this config.
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          The servers here are already lean or are not in the catalog. Nothing to reclaim.
        </p>
      </div>
    );
  }

  if (!hasSavings) {
    return (
      <div className="rounded-lg border border-lime-300/30 bg-lime-300/[0.05] p-4">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-lime-300/90">
          reclaimable per cold session (estimate)
        </p>
        <p className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <span className="font-display text-3xl font-semibold tabular-nums text-lime-300">
            ~{slimTokens.toLocaleString()}
          </span>
          <span className="text-sm text-muted-foreground">tokens by slimming</span>
        </p>
        <p className="mt-2 text-xs text-muted-foreground">
          Keep {slimmable === 1 ? "this server" : `these ${slimmable} servers`} but register fewer
          tools. Snippets are in the table below.
        </p>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-lime-300/30 bg-lime-300/[0.05] p-4">
      <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-lime-300/90">
        reclaimable per cold session
      </p>
      <p className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="font-display text-3xl font-semibold tabular-nums text-lime-300">
          {savingsTokens.toLocaleString()}
        </span>
        <span className="text-sm text-muted-foreground">tokens</span>
        <span className="font-display text-2xl font-semibold tabular-nums text-lime-300">
          {formatUsd(savingsUsd)}
        </span>
        <span className="text-sm text-muted-foreground">at {PROVIDER_LABELS[provider]} rates</span>
      </p>
      <p className="mt-2 text-xs text-muted-foreground">
        Swap {swappable} {swappable === 1 ? "server" : "servers"} for an official CLI to reclaim
        this context.
      </p>
      {hasSlim ? (
        <p className="mt-1 text-xs text-muted-foreground">
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
    return <EmptyHint>No servers found in this config.</EmptyHint>;
  }

  return (
    <div className="overflow-hidden rounded-lg border border-white/10">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-white/10 bg-white/[0.03] font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
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
      <tr className="border-b border-white/5 last:border-b-0">
        <td className="px-3 py-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-foreground">{server.display || server.name}</span>
            <RecommendationBadge recommendation={server.recommendation} />
            {slim != null ? (
              slim.already_slimmed ? (
                <span className="rounded border border-white/10 bg-white/[0.03] px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
                  slimmed
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => setSlimOpen((open) => !open)}
                  aria-expanded={slimOpen}
                  className="rounded border border-sky-400/30 bg-sky-400/10 px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] text-sky-300 transition-colors duration-150 ease-out hover:bg-sky-400/15 focus:outline-none focus-visible:ring-2 focus-visible:ring-sky-400/60"
                >
                  slim available {slimOpen ? "▾" : "▸"}
                </button>
              )
            ) : null}
          </div>
        </td>
        <td className="px-3 py-2 text-right font-mono tabular-nums text-muted-foreground">
          {server.tools ?? "-"}
        </td>
        <td className="px-3 py-2 text-right font-mono tabular-nums text-muted-foreground">
          {server.cold_tokens != null ? server.cold_tokens.toLocaleString() : "-"}
        </td>
        <td className="px-3 py-2 font-mono text-foreground">{server.cli_alternative ?? "-"}</td>
        <td className="px-3 py-2 text-right font-mono tabular-nums">
          {server.savings_tokens > 0 ? (
            <span className="text-lime-300">{server.savings_tokens.toLocaleString()}</span>
          ) : (
            <span className="text-muted-foreground/60">-</span>
          )}
        </td>
      </tr>
      {slim != null && !slim.already_slimmed && slimOpen ? (
        <tr className="border-b border-white/5 last:border-b-0">
          <td colSpan={5} className="px-3 pb-3">
            <SlimDetails slim={slim} />
          </td>
        </tr>
      ) : null}
      {/* Catalog notes render for every matched server (keep servers carry the
          no-native-filtering guidance); unknown servers keep their generic note
          out of the table. */}
      {server.note && server.matched_id != null ? (
        <tr className="border-b border-white/5 last:border-b-0">
          <td colSpan={5} className="px-3 pb-2 text-[11px] leading-5 text-muted-foreground">
            {server.note}
          </td>
        </tr>
      ) : null}
    </>
  );
}

function SlimDetails({ slim }: { slim: McpSlimRecommendation }) {
  return (
    <div className="space-y-2 rounded-md border border-sky-400/30 bg-sky-400/[0.05] p-3">
      <p className="text-[11px] leading-5 text-sky-100">
        <span className="font-medium">{slim.option.mechanism}</span>
        {slim.est_tokens_saved > 0 ? (
          <span className="text-sky-200/80">
            {" "}
            saves an estimated ~{slim.est_tokens_saved.toLocaleString()} input tokens if you keep
            this server.
          </span>
        ) : null}
      </p>
      <div className="flex items-start gap-2">
        <pre className="min-w-0 flex-1 overflow-x-auto rounded bg-black/60 px-3 py-2 font-mono text-[11px] leading-5 text-foreground">
          {slim.option.snippet}
        </pre>
        <CopyButton text={slim.option.snippet} />
      </div>
      <p className="text-[11px] leading-5 text-muted-foreground">{slim.option.note}</p>
      <p className="text-[10px] text-muted-foreground/70">{slim.option.source_hint}</p>
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
      className="shrink-0 rounded-md border border-white/10 bg-white/[0.03] px-2 py-1 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
    >
      {copied ? "copied" : "copy"}
    </button>
  );
}

function RecommendationBadge({ recommendation }: { recommendation: Recommendation }) {
  const styles: Record<Recommendation, string> = {
    replace: "border-lime-300/30 bg-lime-300/10 text-lime-200",
    "replace-for-ad-hoc": "border-amber-400/30 bg-amber-400/10 text-amber-300",
    keep: "border-sky-400/30 bg-sky-400/10 text-sky-300",
    unknown: "border-white/10 bg-white/[0.03] text-muted-foreground",
  };
  const labels: Record<Recommendation, string> = {
    replace: "replace",
    "replace-for-ad-hoc": "ad-hoc",
    keep: "keep",
    unknown: "unknown",
  };
  return (
    <span
      className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] ${styles[recommendation]}`}
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
      <span className="block font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
        {label}
      </span>
      <span className="block font-mono text-base tabular-nums text-foreground">{display}</span>
    </div>
  );
}

function ErrorBox({ message }: { message: string }) {
  return (
    <div className="space-y-1 rounded-lg border border-amber-400/30 bg-amber-400/10 p-4">
      <p className="text-xs font-medium text-amber-300">Could not parse this config.</p>
      <p className="font-mono text-[11px] leading-5 text-amber-300/80">{message}</p>
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
