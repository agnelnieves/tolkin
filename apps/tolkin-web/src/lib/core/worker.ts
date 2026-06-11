/// <reference lib="webworker" />

// Dedicated worker for the tolkin-core WASM module. The core holds the secret
// redactor, the cost calculator, and the pricing tables. Running it off the UI
// thread keeps redaction (a regex pass that can run over megabytes of pasted
// text) from blocking input, and keeps the raw text the user paste off the main
// thread heap: only this worker and the calling component ever hold it.
//
// The module is instantiated at most once, on the first request, then reused.
//
// Protocol: the main thread posts a WorkerRequest with a unique `id`; the worker
// posts back exactly one WorkerResponse carrying the same `id`. The worker stays
// thin: every core function already returns a JSON string, so we forward that
// raw string and let the client parse it into a typed shape. Errors are reported
// in-band as { ok: false } rather than thrown, so the client can reject the
// matching promise without an unhandled "error" event.

export type WorkerOp =
  | "redact"
  | "cost"
  | "models"
  | "analyze-mcp"
  | "parse-tools-list"
  | "analyze-tools-inventory"
  | "audit"
  | "prices-observed";

export type WorkerRequest =
  | { id: number; op: "redact"; text: string; optionsJson: string }
  | { id: number; op: "cost"; requestJson: string }
  | { id: number; op: "models" }
  | { id: number; op: "analyze-mcp"; configText: string; provider: string }
  | { id: number; op: "parse-tools-list"; toolsJson: string }
  | { id: number; op: "analyze-tools-inventory"; requestJson: string }
  | { id: number; op: "audit"; text: string; optionsJson: string }
  | { id: number; op: "prices-observed" };

// Per-variant Omit of the `id` field. A plain Omit<WorkerRequest, "id"> over a
// union collapses to only the shared keys, so this distributes instead, letting
// the client construct a fully typed request body before the id is stamped on.
export type WorkerRequestBody = WorkerRequest extends infer T
  ? T extends { id: number }
    ? Omit<T, "id">
    : never
  : never;

export type WorkerResponse =
  | { id: number; ok: true; result: string }
  | { id: number; ok: false; error: string };

type CoreModule = {
  default: (module_or_path?: unknown) => Promise<unknown>;
  version: () => string;
  redact: (text: string, options_json: string) => string;
  cost: (request_json: string) => string;
  models: () => string;
  analyze_mcp: (config_text: string, provider: string) => string;
  parse_tools_list: (tools_json: string) => string;
  analyze_tools_inventory: (request_json: string) => string;
  audit: (text: string, options_json: string) => string;
  prices_observed: () => string;
};

const ctx = self as unknown as DedicatedWorkerGlobalScope;

let corePromise: Promise<CoreModule> | null = null;

// Lazy-init the WASM module exactly once. Mirrors core-version.tsx: dynamic
// import, then call the default init() before any exported function. wasm-pack's
// `--target web` glue fetches the .wasm asset relative to its own module URL,
// which Turbopack rewrites correctly inside a module worker.
function loadCore(): Promise<CoreModule> {
  if (corePromise) return corePromise;
  corePromise = (async () => {
    const mod = (await import("tolkin-core-wasm")) as unknown as CoreModule;
    await mod.default();
    return mod;
  })();
  return corePromise;
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

function run(req: WorkerRequest, core: CoreModule): string {
  switch (req.op) {
    case "redact":
      return core.redact(req.text, req.optionsJson);
    case "cost":
      return core.cost(req.requestJson);
    case "models":
      return core.models();
    case "analyze-mcp":
      return core.analyze_mcp(req.configText, req.provider);
    case "parse-tools-list":
      return core.parse_tools_list(req.toolsJson);
    case "analyze-tools-inventory":
      return core.analyze_tools_inventory(req.requestJson);
    case "audit":
      return core.audit(req.text, req.optionsJson);
    case "prices-observed":
      return core.prices_observed();
  }
}

ctx.addEventListener("message", (event: MessageEvent<WorkerRequest>) => {
  const req = event.data;
  void (async () => {
    try {
      const core = await loadCore();
      const result = run(req, core);
      ctx.postMessage({ id: req.id, ok: true, result } satisfies WorkerResponse);
    } catch (e) {
      ctx.postMessage({ id: req.id, ok: false, error: errorMessage(e) } satisfies WorkerResponse);
    }
  })();
});
