"use client";

import { Fragment, useEffect, useRef, useState } from "react";
import type { Provider, Segment } from "../lib/tokenize";
import { segments } from "../lib/tokenize";

// Hard cap on rendered chips. Past this, the DOM cost of one span-per-token
// stops being worth it; we render the first CHIP_CAP and note the remainder.
const CHIP_CAP = 2000;

const MIDDOT = "·"; // faint stand-in for a space
const RETURN_GLYPH = "↵"; // small "return" mark shown before a real wrap

type SegState = {
  exact: boolean;
  tokens: number;
  segments: Segment[];
  loading: boolean;
  error: string | null;
};

const initial: SegState = {
  exact: true,
  tokens: 0,
  segments: [],
  loading: false,
  error: null,
};

const TABS: { id: Provider; label: string }[] = [
  { id: "openai", label: "OpenAI" },
  { id: "anthropic", label: "Anthropic" },
  { id: "gemini", label: "Gemini" },
];

export function Visualizer({ text }: { text: string }) {
  const [provider, setProvider] = useState<Provider>("openai");
  const [state, setState] = useState<SegState>(initial);
  const runRef = useRef(0);

  useEffect(() => {
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setState((s) => ({ ...s, loading: true, error: null }));
      segments(provider, text).then(
        (r) => {
          if (runRef.current !== runId) return;
          setState({
            exact: r.exact,
            tokens: r.tokens,
            segments: r.segments,
            loading: false,
            error: null,
          });
        },
        (e: unknown) => {
          if (runRef.current !== runId) return;
          setState({
            exact: true,
            tokens: 0,
            segments: [],
            loading: false,
            error: errorMessage(e),
          });
        },
      );
    }, 100);
    return () => clearTimeout(handle);
  }, [provider, text]);

  return (
    <section className="w-full space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-medium text-zinc-300">Token visualizer</h2>
        <div
          className="inline-flex rounded-lg border border-zinc-800 bg-zinc-900/40 p-0.5"
          role="tablist"
          aria-label="Tokenizer provider"
        >
          {TABS.map((tab) => {
            const active = provider === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setProvider(tab.id)}
                className={
                  active
                    ? "rounded-md bg-zinc-800 px-3 py-1 text-xs font-medium text-zinc-100"
                    : "rounded-md px-3 py-1 text-xs font-medium text-zinc-500 hover:text-zinc-300"
                }
              >
                {tab.label}
              </button>
            );
          })}
        </div>
      </div>

      {provider === "anthropic" ? (
        <AnthropicView tokens={state.tokens} loading={state.loading} error={state.error} />
      ) : (
        <ChipView
          text={text}
          tokens={state.tokens}
          segments={state.segments}
          loading={state.loading}
          error={state.error}
        />
      )}
    </section>
  );
}

function ChipView({
  text,
  tokens,
  segments,
  loading,
  error,
}: {
  text: string;
  tokens: number;
  segments: Segment[];
  loading: boolean;
  error: string | null;
}) {
  const chars = text.length;
  const ratio = tokens > 0 ? (chars / tokens).toFixed(2) : "-";
  const shown = Math.min(segments.length, CHIP_CAP);
  const capped = segments.length > CHIP_CAP;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1 text-xs text-zinc-500">
        <Stat label="tokens" value={tokens.toLocaleString()} />
        <Stat label="chars" value={chars.toLocaleString()} />
        <Stat label="chars / token" value={ratio} />
      </div>

      <div className="min-h-24 rounded-lg border border-zinc-800 bg-zinc-950 px-4 py-3 font-mono text-sm leading-7 text-zinc-100">
        {error ? (
          <span className="text-sm text-red-400">{error}</span>
        ) : loading && segments.length === 0 ? (
          <span className="text-sm text-zinc-500">loading...</span>
        ) : segments.length === 0 ? (
          <span className="text-sm text-zinc-600">
            Nothing to tokenize yet. Paste or drop a file above.
          </span>
        ) : (
          segments.slice(0, shown).map((seg, i) => <Chip key={i} index={i} seg={seg} />)
        )}
      </div>

      {capped ? (
        <p className="text-xs text-zinc-500">
          Showing first {CHIP_CAP.toLocaleString()} of {segments.length.toLocaleString()} tokens.
        </p>
      ) : null}
    </div>
  );
}

// One token chip. The display text is split into space / newline / other runs so
// whitespace is visible: spaces become a faint middot, a newline becomes a small
// return glyph followed by a real line break (the chip wraps via pre-wrap). The
// chip keeps its tinted background across all runs.
function Chip({ index, seg }: { index: number; seg: Segment }) {
  const tone = index % 6;
  const runs = splitRuns(seg.text);
  return (
    <span className={`tok-chip tok-chip-${tone}`} title={`#${index} · id ${seg.id}`}>
      {runs.map((run, i) => {
        if (run.kind === "space") {
          return (
            <span key={i} className="tok-space">
              {MIDDOT.repeat(run.value.length)}
            </span>
          );
        }
        if (run.kind === "newline") {
          return (
            <Fragment key={i}>
              {run.value.split("\n").map((_, n, arr) =>
                n < arr.length - 1 ? (
                  <Fragment key={n}>
                    <span className="tok-newline">{RETURN_GLYPH}</span>
                    {"\n"}
                  </Fragment>
                ) : null,
              )}
            </Fragment>
          );
        }
        return <Fragment key={i}>{run.value}</Fragment>;
      })}
    </span>
  );
}

type Run = { kind: "space" | "newline" | "text"; value: string };

// Split a token's text into consecutive runs of spaces, newlines, or other
// characters so each can be styled independently. Tabs and other whitespace are
// treated as "text" (rendered literally); spaces and newlines are the cases that
// need a visible marker.
function splitRuns(text: string): Run[] {
  const runs: Run[] = [];
  let current: Run | null = null;
  for (const ch of text) {
    const kind: Run["kind"] = ch === "\n" ? "newline" : ch === " " ? "space" : "text";
    if (current && current.kind === kind) {
      current.value += ch;
    } else {
      current = { kind, value: ch };
      runs.push(current);
    }
  }
  return runs;
}

function AnthropicView({
  tokens,
  loading,
  error,
}: {
  tokens: number;
  loading: boolean;
  error: string | null;
}) {
  // +/- 10% band around the cl100k_base estimate, matching the documented
  // approximation error.
  const low = Math.round(tokens * 0.9);
  const high = Math.round(tokens * 1.1);

  return (
    <div className="space-y-4 rounded-lg border border-zinc-800 bg-zinc-900/40 p-5">
      {error ? (
        <p className="text-sm text-red-400">{error}</p>
      ) : (
        <>
          <div>
            <p className="text-xs uppercase tracking-wider text-zinc-500">Estimated tokens</p>
            <p className="mt-1 text-3xl font-semibold tabular-nums text-zinc-100">
              {loading && tokens === 0 ? (
                <span className="text-base font-normal text-zinc-500">loading...</span>
              ) : (
                <>
                  <span className="text-zinc-500">~ </span>
                  {tokens.toLocaleString()}
                </>
              )}
            </p>
          </div>

          <div>
            <p className="text-xs uppercase tracking-wider text-zinc-500">Confidence band</p>
            <p className="mt-1 font-mono text-lg tabular-nums text-zinc-200">
              {low.toLocaleString()} - {high.toLocaleString()}
            </p>
            <p className="mt-1 text-xs text-zinc-500">estimate via cl100k_base proxy</p>
          </div>
        </>
      )}

      <p className="border-t border-zinc-800 pt-4 text-xs leading-5 text-zinc-400">
        Anthropic has not published its tokenizer, so Tolkin does not show fabricated per-token
        boundaries for Claude. The number above is a labeled estimate, not an exact count.
      </p>

      <button
        type="button"
        disabled
        className="cursor-not-allowed rounded-md border border-zinc-800 bg-zinc-900 px-3 py-2 text-xs font-medium text-zinc-500"
        title="Available in Phase 2"
      >
        Verify exact count with Anthropic (Phase 2)
      </button>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-baseline gap-1.5">
      <span className="font-mono tabular-nums text-zinc-200">{value}</span>
      <span>{label}</span>
    </span>
  );
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
