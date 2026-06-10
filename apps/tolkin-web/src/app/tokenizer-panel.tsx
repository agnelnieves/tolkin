"use client";

import { useEffect, useRef, useState } from "react";
import { count } from "../lib/tokenize";
import { AnthropicCard } from "./verify-anthropic";

type CountState = {
  value: number | null;
  loading: boolean;
  error: string | null;
};

const initial: CountState = { value: null, loading: false, error: null };

// The shared input text now lives in the analyzer wrapper and is passed down so
// the count panel and the visualizer stay in sync. Behavior is unchanged: 100ms
// debounce, per-provider loading/error states, stale-run cancellation via the
// run-id ref.
export function TokenizerPanel({ text }: { text: string }) {
  const [openai, setOpenai] = useState<CountState>(initial);
  const [anthropic, setAnthropic] = useState<CountState>(initial);
  const [gemini, setGemini] = useState<CountState>(initial);
  const runRef = useRef(0);

  useEffect(() => {
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setOpenai((s) => ({ ...s, loading: true, error: null }));
      setAnthropic((s) => ({ ...s, loading: true, error: null }));
      setGemini((s) => ({ ...s, loading: true, error: null }));

      // OpenAI is synchronous through the per-encoding entry point; awaiting is
      // still cheap and keeps the three call sites symmetric.
      count("openai", text).then(
        (r) => {
          if (runRef.current === runId) setOpenai({ value: r.tokens, loading: false, error: null });
        },
        (e: unknown) => {
          if (runRef.current === runId)
            setOpenai({ value: null, loading: false, error: errorMessage(e) });
        },
      );

      count("anthropic", text).then(
        (r) => {
          if (runRef.current === runId)
            setAnthropic({ value: r.tokens, loading: false, error: null });
        },
        (e: unknown) => {
          if (runRef.current === runId)
            setAnthropic({ value: null, loading: false, error: errorMessage(e) });
        },
      );

      count("gemini", text).then(
        (r) => {
          if (runRef.current === runId) setGemini({ value: r.tokens, loading: false, error: null });
        },
        (e: unknown) => {
          if (runRef.current === runId)
            setGemini({ value: null, loading: false, error: errorMessage(e) });
        },
      );
    }, 100);
    return () => clearTimeout(handle);
  }, [text]);

  const chars = text.length;

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <ProviderCard name="OpenAI" subtitle="o200k_base, exact" state={openai} chars={chars} />
      <AnthropicCard text={text} state={anthropic} />
      <ProviderCard name="Gemini" subtitle="Gemma SPM, exact" state={gemini} chars={chars} />
    </div>
  );
}

// The Anthropic card has its own component (see ./verify-anthropic) because it
// carries the estimate presentation plus the opt-in verify flow.
function ProviderCard({
  name,
  subtitle,
  state,
  chars,
}: {
  name: string;
  subtitle: string;
  state: CountState;
  chars: number;
}) {
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium text-zinc-300">{name}</h2>
        <span className="text-[10px] uppercase tracking-wider text-zinc-500">
          {chars.toLocaleString()} chars
        </span>
      </div>
      <p className="mt-3 text-3xl font-semibold tabular-nums text-zinc-100">
        {state.error ? (
          <span className="text-base font-normal text-red-400">error</span>
        ) : state.value === null && state.loading ? (
          <span className="text-base font-normal text-zinc-500">loading...</span>
        ) : state.value === null ? (
          <span className="text-base font-normal text-zinc-500">-</span>
        ) : (
          <>
            {state.value.toLocaleString()}
            <span className="ml-2 text-sm font-normal text-zinc-500">tokens</span>
          </>
        )}
      </p>
      <p className="mt-1 text-xs text-zinc-500">{state.error ?? subtitle}</p>
    </div>
  );
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
