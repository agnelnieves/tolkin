// Typed main-thread client for the compression worker. Mirrors the tokenize
// client (lazy worker creation guarded against SSR, id-correlated replies),
// plus a small observable model state so the panel can render idle ->
// downloading (with percent) -> ready -> error without polling.

import type { CompressOutput, ModelState } from "./types";
import type { CompressWorkerRequest, CompressWorkerResponse } from "./worker";

type Pending = {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
};

let worker: Worker | null = null;
const pending = new Map<number, Pending>();
let nextId = 0;

let state: ModelState = { status: "idle", progress: 0, error: null };
const listeners = new Set<(s: ModelState) => void>();
let loadPromise: Promise<void> | null = null;

function setState(next: ModelState): void {
  state = next;
  for (const listener of listeners) listener(state);
}

export function modelState(): ModelState {
  return state;
}

export function subscribeModelState(listener: (s: ModelState) => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getWorker(): Worker {
  if (typeof window === "undefined") {
    throw new Error("Compression worker is only available in the browser.");
  }
  if (worker) return worker;
  worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  worker.addEventListener("message", (event: MessageEvent<CompressWorkerResponse>) => {
    const data = event.data;
    if (data.kind === "progress") {
      if (state.status === "downloading") {
        setState({ status: "downloading", progress: data.percent, error: null });
      }
      return;
    }
    const entry = pending.get(data.id);
    if (!entry) return;
    pending.delete(data.id);
    if (data.ok) {
      entry.resolve("result" in data ? data.result : undefined);
    } else {
      entry.reject(new Error(data.error));
    }
  });
  worker.addEventListener("error", (event: ErrorEvent) => {
    // A hard worker failure rejects every in-flight request; the next call
    // re-creates the worker.
    const error = new Error(event.message || "Compression worker error");
    for (const [, entry] of pending) entry.reject(error);
    pending.clear();
    worker = null;
    setState({ status: "error", progress: 0, error: error.message });
  });
  return worker;
}

// Omit over the request union would collapse to the common members, so the
// id-less body is spelled out instead.
type RequestBody = { op: "load" } | { op: "compress"; text: string; rate: number };

function request<T>(message: RequestBody): Promise<T> {
  const w = getWorker();
  const id = nextId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    w.postMessage({ ...message, id } satisfies CompressWorkerRequest);
  });
}

// Idempotent: concurrent callers share one in-flight load, and a completed
// load resolves immediately. A failed load clears the shared promise so the
// retry button can attempt a fresh download.
export function loadModel(): Promise<void> {
  if (state.status === "ready") return Promise.resolve();
  if (loadPromise) return loadPromise;
  setState({ status: "downloading", progress: 0, error: null });
  loadPromise = request<undefined>({ op: "load" }).then(
    () => {
      setState({ status: "ready", progress: 100, error: null });
    },
    (e: unknown) => {
      loadPromise = null;
      const message = e instanceof Error ? e.message : String(e);
      setState({ status: "error", progress: 0, error: message });
      throw e instanceof Error ? e : new Error(message);
    },
  );
  return loadPromise;
}

export async function compress(text: string, rate: number): Promise<CompressOutput> {
  // The worker lazy-loads the model on first compress regardless, but going
  // through loadModel() keeps the observable state truthful for the panel.
  await loadModel();
  return request<CompressOutput>({ op: "compress", text, rate });
}

// Terminating the worker is the only reliable way to drop its heap (model
// weights and any text it has seen). The next loadModel() starts clean.
export function terminate(): void {
  if (worker) {
    worker.terminate();
    worker = null;
  }
  const error = new Error("Compression worker terminated.");
  for (const [, entry] of pending) entry.reject(error);
  pending.clear();
  loadPromise = null;
  setState({ status: "idle", progress: 0, error: null });
}
