"use client";

import { useEffect, useRef, useState } from "react";
import { PanelCard, PanelHeader } from "@/components/analyzer/panel";
import type { ModelState } from "../lib/compress";
import { compress, loadModel, modelState, subscribeModelState } from "../lib/compress";
import { count } from "../lib/tokenize";

// Opt-in LLMLingua-2 compression preview. Everything is gated behind an
// explicit "Load model" click: no worker, no download, no inference happens
// before that. The model is a static fetch from the Hugging Face Hub; the
// text being compressed never leaves the worker.

const RATE_OPTIONS = [
  { value: 0.7, name: "Conservative", note: "keeps ~70% of input tokens" },
  { value: 0.5, name: "Balanced", note: "keeps ~50% of input tokens" },
  {
    value: 0.33,
    name: "Aggressive",
    note: "keeps ~33%; can increase total cost: compressed prompts may grow outputs",
  },
] as const;

type ResultState = {
  original: string;
  compressed: string;
  // Exact o200k_base counts from the shared tokenize worker; null while the
  // counts are still being computed.
  originalTokens: number | null;
  compressedTokens: number | null;
};

export function CompressPanel({ text }: { text: string }) {
  const [model, setModel] = useState<ModelState>(() => modelState());
  const [rate, setRate] = useState<number>(0.5);
  const [compressing, setCompressing] = useState(false);
  const [compressError, setCompressError] = useState<string | null>(null);
  const [result, setResult] = useState<ResultState | null>(null);
  const [copied, setCopied] = useState(false);
  const runRef = useRef(0);

  useEffect(() => subscribeModelState(setModel), []);

  function onLoadModel() {
    loadModel().catch(() => {
      // The error is surfaced through the subscribed model state.
    });
  }

  function onCompress() {
    const runId = ++runRef.current;
    setCompressing(true);
    setCompressError(null);
    setCopied(false);
    compress(text, rate).then(
      (out) => {
        if (runRef.current !== runId) return;
        setCompressing(false);
        setResult({
          original: text,
          compressed: out.compressed,
          originalTokens: null,
          compressedTokens: null,
        });
        // Exact before/after counts via the shared OpenAI tokenizer worker.
        Promise.all([count("openai", text), count("openai", out.compressed)]).then(
          ([before, after]) => {
            if (runRef.current !== runId) return;
            setResult((r) =>
              r ? { ...r, originalTokens: before.tokens, compressedTokens: after.tokens } : r,
            );
          },
          () => {
            // Counting failed; the result stays visible without numbers.
          },
        );
      },
      (e: unknown) => {
        if (runRef.current !== runId) return;
        setCompressing(false);
        setCompressError(e instanceof Error ? e.message : String(e));
      },
    );
  }

  function onCopy() {
    if (!result) return;
    navigator.clipboard.writeText(result.compressed).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      },
      () => {
        setCompressError("Could not copy to the clipboard.");
      },
    );
  }

  const savedPercent =
    result && result.originalTokens && result.compressedTokens !== null && result.originalTokens > 0
      ? Math.round(
          ((result.originalTokens - result.compressedTokens) / result.originalTokens) * 100,
        )
      : null;

  return (
    <PanelCard>
      <PanelHeader
        title="Compression preview"
        blurb="Optional. Trim low-information words while keeping the load-bearing ones. Lossy. Off by default."
        status="experimental, lossy"
      />

      {model.status !== "ready" ? (
        <div className="space-y-3 rounded-lg border border-white/10 bg-black/30 p-4">
          <p className="text-xs leading-5 text-muted-foreground">
            Compression preview (experimental, lossy). Downloads a ~58 MB model to your browser on
            first use. Runs fully locally; nothing leaves your browser.
          </p>
          <p className="text-xs leading-5 text-muted-foreground">
            Powered by LLMLingua-2 (TinyBERT variant). The model drops low-information words while
            keeping the load-bearing ones. This variant is uncased: compressed output is lowercased.
          </p>

          {model.status === "downloading" ? (
            <div className="space-y-1">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/[0.05]">
                <div
                  className="h-full rounded-full bg-lime-300 transition-[width] duration-300 ease-out"
                  style={{ width: `${model.progress}%` }}
                />
              </div>
              <p className="font-mono text-[11px] tabular-nums text-muted-foreground">
                Downloading model. {model.progress}%
              </p>
            </div>
          ) : model.status === "error" ? (
            <div className="space-y-2">
              <p className="text-xs leading-5 text-destructive">
                Model failed to load: {model.error ?? "unknown error"}
              </p>
              <button
                type="button"
                onClick={onLoadModel}
                className="min-h-11 rounded-md border border-white/10 bg-white/[0.03] px-4 py-2 text-xs font-medium text-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
              >
                Retry
              </button>
            </div>
          ) : (
            <button
              type="button"
              onClick={onLoadModel}
              className="inline-flex min-h-11 items-center justify-center rounded-md border border-lime-300/40 bg-lime-300/10 px-4 py-2 text-xs font-medium text-lime-200 transition-colors duration-150 ease-out hover:bg-lime-300/15 focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
            >
              Load model (~58 MB)
            </button>
          )}
        </div>
      ) : (
        <div className="space-y-4 rounded-lg border border-white/10 bg-black/30 p-4">
          <div className="flex flex-wrap items-center gap-2">
            {RATE_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                onClick={() => setRate(option.value)}
                title={option.note}
                className={`min-h-11 rounded-md border px-3 py-2 font-mono text-xs font-medium transition-colors duration-150 ease-out focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60 ${
                  rate === option.value
                    ? "border-lime-300/40 bg-lime-300/10 text-lime-200"
                    : "border-white/10 bg-white/[0.02] text-muted-foreground hover:border-white/25 hover:text-foreground"
                }`}
              >
                {option.name} {option.value}
              </button>
            ))}
          </div>
          <p className="text-xs leading-5 text-muted-foreground">
            {RATE_OPTIONS.find((o) => o.value === rate)?.note}
          </p>

          <button
            type="button"
            onClick={onCompress}
            disabled={compressing || text.trim().length === 0}
            className="inline-flex min-h-11 items-center justify-center rounded-md border border-lime-300/40 bg-lime-300/10 px-4 py-2 text-xs font-medium text-lime-200 transition-colors duration-150 ease-out hover:bg-lime-300/15 focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {compressing ? "Compressing." : "Compress"}
          </button>

          {compressError ? (
            <p className="text-xs leading-5 text-destructive">{compressError}</p>
          ) : null}

          {result ? (
            <div className="space-y-3">
              <div className="space-y-1">
                <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
                  Original{" "}
                  {result.originalTokens !== null
                    ? `(${result.originalTokens.toLocaleString()} tokens, o200k_base exact)`
                    : ""}
                </p>
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-white/10 bg-black/40 px-3 py-2 font-mono text-xs leading-5 text-muted-foreground">
                  {result.original}
                </pre>
              </div>

              <div className="space-y-1">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
                    Compressed{" "}
                    {result.compressedTokens !== null
                      ? `(${result.compressedTokens.toLocaleString()} tokens, o200k_base exact${
                          savedPercent !== null ? `, ${savedPercent}% fewer` : ""
                        })`
                      : ""}
                  </p>
                  <button
                    type="button"
                    onClick={onCopy}
                    className="rounded-md border border-white/10 bg-white/[0.03] px-2 py-1 text-[11px] text-muted-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
                  >
                    {copied ? "Copied" : "Copy"}
                  </button>
                </div>
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-white/10 bg-black/40 px-3 py-2 font-mono text-xs leading-5 text-foreground">
                  {result.compressed}
                </pre>
              </div>

              <p className="rounded-md border border-amber-400/30 bg-amber-400/5 px-3 py-2 text-xs leading-5 text-amber-300/90">
                Lossy compression. Verify quality on your task before adopting. Savings are
                input-token only;{" "}
                <a
                  href="https://arxiv.org/abs/2603.23525"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline decoration-amber-400/60 underline-offset-2 hover:text-amber-200 hover:decoration-amber-300"
                >
                  a March 2026 randomized trial
                </a>{" "}
                found aggressive compression can increase total cost.
              </p>
            </div>
          ) : null}
        </div>
      )}
    </PanelCard>
  );
}
