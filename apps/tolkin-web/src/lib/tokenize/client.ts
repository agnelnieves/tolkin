// Typed main-thread client for the tokenization worker. Exposes count() and
// segments() as promises. One worker instance is created lazily on first use
// (guarded so it never runs during SSR) and reused for every request. Replies
// are correlated with requests by an incrementing id stored in a pending map.

import type { Provider, SegmentResult, TokenCount } from "./types";
import type { WorkerOp, WorkerRequest, WorkerResponse } from "./worker";

type Pending = {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
};

let worker: Worker | null = null;
const pending = new Map<number, Pending>();
let nextId = 0;

function getWorker(): Worker {
  if (typeof window === "undefined") {
    // Web Workers do not exist during server rendering. Callers are client
    // components, so this only guards the SSR/prerender pass.
    throw new Error("Tokenization worker is only available in the browser.");
  }
  if (worker) return worker;
  worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  worker.addEventListener("message", (event: MessageEvent<WorkerResponse>) => {
    const data = event.data;
    const entry = pending.get(data.id);
    if (!entry) return;
    pending.delete(data.id);
    if (data.ok) {
      entry.resolve(data.result);
    } else {
      entry.reject(new Error(data.error));
    }
  });
  worker.addEventListener("error", (event: ErrorEvent) => {
    // A hard worker failure rejects every in-flight request; the next call
    // re-creates the worker.
    const error = new Error(event.message || "Tokenization worker error");
    for (const [, entry] of pending) entry.reject(error);
    pending.clear();
    worker = null;
  });
  return worker;
}

function request<T>(op: WorkerOp, provider: Provider, text: string): Promise<T> {
  const w = getWorker();
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    w.postMessage({ id, op, provider, text } satisfies WorkerRequest);
  });
}

export function count(provider: Provider, text: string): Promise<TokenCount> {
  return request<TokenCount>("count", provider, text);
}

export function segments(provider: Provider, text: string): Promise<SegmentResult> {
  return request<SegmentResult>("segments", provider, text);
}
