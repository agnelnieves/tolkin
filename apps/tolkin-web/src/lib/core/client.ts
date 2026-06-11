// Typed main-thread client for the tolkin-core worker. Exposes redact(),
// cost(), and models() as promises over the typed shapes in ./types. One worker
// instance is created lazily on first use (guarded so it never runs during SSR)
// and reused for every request. Replies are correlated with requests by an
// incrementing id stored in a pending map. The worker forwards raw JSON strings;
// this client is where JSON.stringify (inputs) and JSON.parse (outputs) happen.

import type {
  AuditOptions,
  AuditReport,
  CoreProvider,
  CostBreakdown,
  CostRequest,
  McpAnalysis,
  McpToolSpec,
  McpToolTokenCount,
  ModelPrice,
  RedactionResult,
  RedactOptions,
} from "./types";
import type { WorkerRequest, WorkerRequestBody, WorkerResponse } from "./worker";

type Pending = {
  resolve: (value: string) => void;
  reject: (reason: Error) => void;
};

let worker: Worker | null = null;
const pending = new Map<number, Pending>();
let nextId = 0;

function getWorker(): Worker {
  if (typeof window === "undefined") {
    // Web Workers do not exist during server rendering. Callers are client
    // components, so this only guards the SSR/prerender pass.
    throw new Error("Core worker is only available in the browser.");
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
    const error = new Error(event.message || "Core worker error");
    for (const [, entry] of pending) entry.reject(error);
    pending.clear();
    worker = null;
  });
  return worker;
}

// Post a request and resolve with the raw JSON string the worker returns.
function request(req: WorkerRequestBody): Promise<string> {
  const w = getWorker();
  const id = nextId++;
  return new Promise<string>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    w.postMessage({ ...req, id } as WorkerRequest);
  });
}

export async function redact(text: string, opts?: RedactOptions): Promise<RedactionResult> {
  // Empty options string tells the core to use its defaults.
  const optionsJson = opts ? JSON.stringify(opts) : "";
  const raw = await request({ op: "redact", text, optionsJson });
  return JSON.parse(raw) as RedactionResult;
}

export async function cost(req: CostRequest): Promise<CostBreakdown> {
  const raw = await request({ op: "cost", requestJson: JSON.stringify(req) });
  return JSON.parse(raw) as CostBreakdown;
}

export async function models(): Promise<ModelPrice[]> {
  const raw = await request({ op: "models" });
  return JSON.parse(raw) as ModelPrice[];
}

// Analyze an MCP config. The core parses the config (JSON or JSONC), matches
// each server against its catalog, and returns the token cost of the tool
// definitions plus swap recommendations. Rejects with a human-readable message
// when the config cannot be parsed. provider drives the cache math only.
export async function analyzeMcp(
  configText: string,
  provider: CoreProvider = "anthropic",
): Promise<McpAnalysis> {
  const raw = await request({ op: "analyze-mcp", configText, provider });
  return JSON.parse(raw) as McpAnalysis;
}

// Parse a tools/list payload ({"tools": [...]}, a bare tool array, or a
// JSON-RPC envelope) into ToolSpecs. The caller tokenizes each `serialized`
// string and each `description` with the panel's own tokenizer, then feeds the
// counts to analyzeToolsInventory. Tokenization never happens in the core.
export async function parseToolsList(toolsJson: string): Promise<McpToolSpec[]> {
  const raw = await request({ op: "parse-tools-list", toolsJson });
  return JSON.parse(raw) as McpToolSpec[];
}

// Analyze a pre-tokenized tool inventory as a single-server McpAnalysis. The
// returned server carries exact tokenized-manifest counts (basis label says
// so) plus the per-tool table and description smells under tools_detail.
export async function analyzeToolsInventory(
  server: string,
  tokenizer: string,
  tools: McpToolTokenCount[],
  provider: CoreProvider = "anthropic",
): Promise<McpAnalysis> {
  const requestJson = JSON.stringify({ server, provider, tokenizer, tools });
  const raw = await request({ op: "analyze-tools-inventory", requestJson });
  return JSON.parse(raw) as McpAnalysis;
}

// Audit text for token waste. The core runs every production-proven rule and
// returns ranked findings with input-token savings ranges. Pass input_tokens in
// options when a real count is available; otherwise the core falls back to a
// bytes/4 approximation and says so in the report notes.
export async function audit(text: string, options?: AuditOptions): Promise<AuditReport> {
  // Empty options string tells the core to use its defaults.
  const optionsJson = options ? JSON.stringify(options) : "";
  const raw = await request({ op: "audit", text, optionsJson });
  return JSON.parse(raw) as AuditReport;
}

// When the vendored pricing tables were last checked (YYYY-MM). Unlike the
// other ops this is a bare string, not JSON, so it is returned as-is.
export async function pricesObserved(): Promise<string> {
  return request({ op: "prices-observed" });
}

// Drop the worker and its WASM heap. The redactor holds the raw pasted text only
// for the duration of a call, but terminate() is the reliable way to release the
// worker entirely when the analyzer unmounts.
export function terminate(): void {
  if (worker) {
    worker.terminate();
    worker = null;
  }
  for (const [, entry] of pending) entry.reject(new Error("Core worker terminated."));
  pending.clear();
}
