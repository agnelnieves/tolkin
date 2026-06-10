"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { CoreProvider, CostBreakdown, ModelPrice } from "../lib/core";
import { cost as coreCost, models as coreModels, pricesObserved } from "../lib/core";
import { count } from "../lib/tokenize";

// Cost calculator. All pricing and cost math come from the WASM core; this
// component only gathers an input-token count per provider (via the tokenize
// client, reused, not reimplemented), feeds the toggles into a CostRequest, and
// renders what the core returns. Every dollar figure is input-token-bounded:
// output cost is either user-supplied or an estimate from the provider's
// output:input ratio, which the UI labels as such.

// The three is_default models, shown side by side in the default comparison.
const DEFAULT_MODEL_IDS = ["gpt-5.4", "claude-sonnet-4.6", "gemini-2.5-pro"];

type CountState = {
  // input-token count per provider, since every model of a provider shares it
  counts: Partial<Record<CoreProvider, number>>;
  loading: boolean;
  error: string | null;
};

type Toggles = {
  cacheHitRate: number;
  outputTokens: string; // empty string means "estimate from ratio"
  calls: string;
  batchFraction: number;
  cacheTtl: "5m" | "1h";
};

const initialToggles: Toggles = {
  cacheHitRate: 0,
  outputTokens: "",
  calls: "1",
  batchFraction: 0,
  cacheTtl: "5m",
};

export function CostPanel({ text }: { text: string }) {
  const [catalog, setCatalog] = useState<ModelPrice[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [observed, setObserved] = useState<string | null>(null);
  const [selected, setSelected] = useState<string>("compare");
  const [toggles, setToggles] = useState<Toggles>(initialToggles);
  const [countState, setCountState] = useState<CountState>({
    counts: {},
    loading: false,
    error: null,
  });
  const [breakdowns, setBreakdowns] = useState<CostBreakdown[]>([]);
  const countRunRef = useRef(0);
  const costRunRef = useRef(0);

  // Load the pricing catalog once on mount, plus the YYYY-MM the vendored
  // prices were last checked. The staleness note is best-effort: a failure
  // just leaves it off, it never blocks the calculator.
  useEffect(() => {
    let cancelled = false;
    coreModels().then(
      (list) => {
        if (!cancelled) setCatalog(list);
      },
      (e: unknown) => {
        if (!cancelled) setCatalogError(errorMessage(e));
      },
    );
    pricesObserved().then(
      (date) => {
        if (!cancelled) setObserved(date);
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, []);

  // The models currently on screen: either the three defaults, or one pick.
  const shownModels = useMemo<ModelPrice[]>(() => {
    if (catalog.length === 0) return [];
    if (selected === "compare") {
      return DEFAULT_MODEL_IDS.map((id) => catalog.find((m) => m.id === id)).filter(
        (m): m is ModelPrice => m !== undefined,
      );
    }
    const one = catalog.find((m) => m.id === selected);
    return one ? [one] : [];
  }, [catalog, selected]);

  // The distinct providers across the shown models. We tokenize once per
  // provider, not once per model.
  const shownProviders = useMemo<CoreProvider[]>(() => {
    const set = new Set<CoreProvider>();
    for (const m of shownModels) set.add(m.provider);
    return [...set];
  }, [shownModels]);

  // Debounced input-token count per shown provider. Mirrors tokenizer-panel's
  // 100ms debounce and run-id stale-guard.
  useEffect(() => {
    if (shownProviders.length === 0) return;
    const handle = setTimeout(() => {
      const runId = ++countRunRef.current;
      setCountState((s) => ({ ...s, loading: true, error: null }));
      Promise.all(
        shownProviders.map(async (p) => [p, (await count(p, text)).tokens] as const),
      ).then(
        (pairs) => {
          if (countRunRef.current !== runId) return;
          const counts: Partial<Record<CoreProvider, number>> = {};
          for (const [p, n] of pairs) counts[p] = n;
          setCountState({ counts, loading: false, error: null });
        },
        (e: unknown) => {
          if (countRunRef.current !== runId) return;
          setCountState({ counts: {}, loading: false, error: errorMessage(e) });
        },
      );
    }, 100);
    return () => clearTimeout(handle);
  }, [shownProviders, text]);

  // Recompute cost whenever the counts, the shown models, or a toggle changes.
  // Cost math is fast (pure core call), but we still guard against stale runs so
  // a slow earlier batch cannot overwrite a newer result.
  useEffect(() => {
    if (shownModels.length === 0) {
      setBreakdowns([]);
      return;
    }
    const calls = clampInt(toggles.calls, 1);
    const outputTokens =
      toggles.outputTokens.trim() === "" ? undefined : clampInt(toggles.outputTokens, 0);
    const runId = ++costRunRef.current;

    Promise.all(
      shownModels.map((m) => {
        const inputTokens = countState.counts[m.provider] ?? 0;
        return coreCost({
          model_id: m.id,
          input_tokens: inputTokens,
          output_tokens: outputTokens,
          calls,
          cache_hit_rate: toggles.cacheHitRate,
          batch_fraction: toggles.batchFraction,
          cache_ttl: toggles.cacheTtl,
        });
      }),
    ).then(
      (results) => {
        if (costRunRef.current === runId) setBreakdowns(results);
      },
      () => {
        if (costRunRef.current === runId) setBreakdowns([]);
      },
    );
  }, [shownModels, countState.counts, toggles]);

  const showCacheSplit = toggles.cacheHitRate > 0;
  // Surface the honesty notes from the first breakdown that has any.
  const notes = breakdowns.find((b) => b.notes.length > 0)?.notes ?? [];

  return (
    <section className="w-full space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-medium text-zinc-300">Cost calculator</h2>
        <label className="flex items-center gap-2 text-xs text-zinc-400">
          <span>Model</span>
          <select
            value={selected}
            onChange={(e) => setSelected(e.target.value)}
            className="rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 focus:border-zinc-600 focus:outline-none"
          >
            <option value="compare">Compare defaults</option>
            {catalog.map((m) => (
              <option key={m.id} value={m.id}>
                {m.display}
              </option>
            ))}
          </select>
        </label>
      </div>

      <Controls toggles={toggles} onChange={setToggles} />

      {catalogError ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-sm text-red-400">
          {catalogError}
        </p>
      ) : countState.error ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-sm text-red-400">
          {countState.error}
        </p>
      ) : (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {breakdowns.map((b) => (
            <CostCard key={b.model_id} breakdown={b} showCacheSplit={showCacheSplit} />
          ))}
          {breakdowns.length === 0 ? <p className="text-xs text-zinc-500">loading...</p> : null}
        </div>
      )}

      <div className="space-y-1 border-t border-zinc-800 pt-3 text-xs leading-5 text-zinc-500">
        <p>Input-token-bounded. Output cost varies by response.</p>
        {observed ? (
          <p>Prices observed {observed}. Verify against provider pricing pages.</p>
        ) : null}
        {notes.map((n) => (
          <p key={n}>{n}</p>
        ))}
      </div>
    </section>
  );
}

function Controls({ toggles, onChange }: { toggles: Toggles; onChange: (next: Toggles) => void }) {
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-3 rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 sm:grid-cols-3 lg:grid-cols-5">
      <Field label="Cache hit rate">
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={toggles.cacheHitRate}
            onChange={(e) => onChange({ ...toggles, cacheHitRate: Number(e.target.value) })}
            className="w-full accent-zinc-400"
          />
          <span className="w-9 shrink-0 text-right text-[11px] tabular-nums text-zinc-400">
            {Math.round(toggles.cacheHitRate * 100)}%
          </span>
        </div>
      </Field>

      <Field label="Output tokens">
        <input
          type="number"
          min={0}
          inputMode="numeric"
          placeholder="estimate"
          value={toggles.outputTokens}
          onChange={(e) => onChange({ ...toggles, outputTokens: e.target.value })}
          className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs tabular-nums text-zinc-200 placeholder:text-zinc-600 focus:border-zinc-600 focus:outline-none"
        />
      </Field>

      <Field label="Calls">
        <input
          type="number"
          min={1}
          inputMode="numeric"
          value={toggles.calls}
          onChange={(e) => onChange({ ...toggles, calls: e.target.value })}
          className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs tabular-nums text-zinc-200 focus:border-zinc-600 focus:outline-none"
        />
      </Field>

      <Field label="Batch fraction">
        <div className="flex items-center gap-2">
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={toggles.batchFraction}
            onChange={(e) => onChange({ ...toggles, batchFraction: Number(e.target.value) })}
            className="w-full accent-zinc-400"
          />
          <span className="w-9 shrink-0 text-right text-[11px] tabular-nums text-zinc-400">
            {Math.round(toggles.batchFraction * 100)}%
          </span>
        </div>
      </Field>

      <Field label="Cache TTL">
        <select
          value={toggles.cacheTtl}
          onChange={(e) => onChange({ ...toggles, cacheTtl: e.target.value as "5m" | "1h" })}
          className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs text-zinc-200 focus:border-zinc-600 focus:outline-none"
        >
          <option value="5m">5m</option>
          <option value="1h">1h</option>
        </select>
      </Field>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="block text-[10px] uppercase tracking-wider text-zinc-500">{label}</span>
      {children}
    </label>
  );
}

function CostCard({
  breakdown,
  showCacheSplit,
}: {
  breakdown: CostBreakdown;
  showCacheSplit: boolean;
}) {
  const isAnthropic = breakdown.provider === "anthropic";
  // Anthropic input counts are an offline estimate (no public tokenizer), so the
  // whole row is marked approximate.
  const inputPrefix = isAnthropic ? "~ " : "";
  const outputPrefix = breakdown.output_estimated ? "~ " : "";

  return (
    <div className="space-y-3 rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="text-sm font-medium text-zinc-300">{breakdown.display}</h3>
        {isAnthropic ? (
          <span className="text-[10px] uppercase tracking-wider text-amber-400/80">estimate</span>
        ) : null}
      </div>

      <div className="space-y-1 text-xs text-zinc-500">
        <Row label="input">
          {inputPrefix}
          {breakdown.input_tokens.toLocaleString()} tok
        </Row>
        <Row label="output">
          {outputPrefix}
          {breakdown.output_tokens.toLocaleString()} tok
        </Row>
        {showCacheSplit ? (
          <Row label="cache">
            {breakdown.fresh_tokens.toLocaleString()} fresh /{" "}
            {breakdown.cached_tokens.toLocaleString()} cached
          </Row>
        ) : null}
      </div>

      <div className="space-y-1 border-t border-zinc-800 pt-3">
        <div className="flex items-baseline justify-between gap-2">
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">per call</span>
          <span className="font-mono text-sm tabular-nums text-zinc-200">
            {formatUsd(breakdown.per_call.total)}
          </span>
        </div>
        <div className="flex items-baseline justify-between gap-2">
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">
            total ({breakdown.calls.toLocaleString()})
          </span>
          <span className="font-mono text-base font-semibold tabular-nums text-zinc-100">
            {formatUsd(breakdown.total.total)}
          </span>
        </div>
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span>{label}</span>
      <span className="font-mono tabular-nums text-zinc-300">{children}</span>
    </div>
  );
}

// Format a dollar amount with enough precision that sub-cent figures stay
// legible. Large values round to cents; small ones keep up to four decimals.
function formatUsd(value: number): string {
  if (value === 0) return "$0";
  if (value >= 1) return `$${value.toFixed(2)}`;
  if (value >= 0.01) return `$${value.toFixed(3)}`;
  return `$${value.toFixed(4)}`;
}

function clampInt(raw: string, fallback: number): number {
  const n = Number.parseInt(raw, 10);
  if (Number.isNaN(n) || n < 0) return fallback;
  return n;
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
