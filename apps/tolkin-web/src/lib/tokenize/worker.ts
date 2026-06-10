/// <reference lib="webworker" />

// Single dedicated tokenization worker. All three tokenizers run here so large
// inputs never block the UI thread, and the ~4 MB Gemma vocab loads at most
// once per page (this module is instantiated a single time by the client).
//
// Protocol: the main thread posts a WorkerRequest with a unique `id`; the
// worker posts back exactly one WorkerResponse carrying the same `id`. Errors
// are reported in-band as { ok: false } rather than thrown, so the client can
// reject the matching promise without an unhandled "error" event.

import * as anthropic from "./anthropic";
import * as gemini from "./gemini";
import * as openai from "./openai";
import type { Provider, SegmentResult, TokenCount } from "./types";

export type WorkerOp = "count" | "segments";

export type WorkerRequest = {
  id: number;
  op: WorkerOp;
  provider: Provider;
  text: string;
};

export type WorkerResponse =
  | { id: number; ok: true; op: "count"; result: TokenCount }
  | { id: number; ok: true; op: "segments"; result: SegmentResult }
  | { id: number; ok: false; error: string };

const ctx = self as unknown as DedicatedWorkerGlobalScope;

async function runCount(provider: Provider, text: string): Promise<TokenCount> {
  switch (provider) {
    case "openai":
      return { provider, tokens: openai.count(text) };
    case "anthropic": {
      const tokens = await anthropic.count(text);
      return {
        provider,
        tokens,
        approximate: true,
        approximateNote: "cl100k_base approximation, +/- 10%",
      };
    }
    case "gemini": {
      const tokens = await gemini.count(text);
      return { provider, tokens };
    }
  }
}

async function runSegments(provider: Provider, text: string): Promise<SegmentResult> {
  switch (provider) {
    case "openai": {
      const { tokens, segments } = openai.segments(text);
      return { provider, exact: true, tokens, segments };
    }
    case "gemini": {
      const { tokens, segments } = await gemini.segments(text);
      return { provider, exact: true, tokens, segments };
    }
    case "anthropic": {
      const { tokens, segments } = await anthropic.segments(text);
      // Never exact: Anthropic's tokenizer is closed, so no real boundaries.
      return { provider, exact: false, tokens, segments };
    }
  }
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

ctx.addEventListener("message", (event: MessageEvent<WorkerRequest>) => {
  const { id, op, provider, text } = event.data;
  void (async () => {
    try {
      if (op === "count") {
        const result = await runCount(provider, text);
        ctx.postMessage({ id, ok: true, op, result } satisfies WorkerResponse);
      } else {
        const result = await runSegments(provider, text);
        ctx.postMessage({ id, ok: true, op, result } satisfies WorkerResponse);
      }
    } catch (e) {
      ctx.postMessage({ id, ok: false, error: errorMessage(e) } satisfies WorkerResponse);
    }
  })();
});
