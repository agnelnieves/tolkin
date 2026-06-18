/* tslint:disable */
/* eslint-disable */

/**
 * Analyze an MCP config (JSON or JSONC) for `provider` ("openai" | "anthropic"
 * | "gemini"; anything else defaults to anthropic). Returns a JSON `McpAnalysis`.
 */
export function analyze_mcp(config_text: string, provider: string): string;

/**
 * Analyze a pre-tokenized tool inventory. `request_json` is a JSON
 * `InventoryRequest`: `{ "server": str, "provider": "openai" | "anthropic" |
 * "gemini" (optional, defaults anthropic), "tokenizer": str, "tools":
 * [{ "name", "description", "tokens", "description_tokens" }] }`. Returns a
 * JSON `McpAnalysis` with one server carrying exact tokenized-manifest counts
 * plus the per-tool table and description smells in `tools_detail`.
 */
export function analyze_tools_inventory(request_json: string): string;

/**
 * Audit `text` for token waste. `options_json` may be empty (or `{}`) for
 * defaults, or a JSON `AuditOptions`
 * (`{ "input_tokens": number | null, "include_experimental": bool }`).
 * Returns a JSON `AuditReport`.
 */
export function audit(text: string, options_json: string): string;

/**
 * Estimate cost. `request_json` is a JSON `CostRequest` (only `model_id` and
 * `input_tokens` are required). Returns a JSON `CostBreakdown`.
 */
export function cost(request_json: string): string;

/**
 * Every pricing model as a JSON array of `ModelPrice`.
 */
export function models(): string;

/**
 * Parse a tools/list payload (the MCP `{"tools": [...]}` result shape, a bare
 * array of tool objects, or a full JSON-RPC envelope) into a JSON array of
 * `ToolSpec` (`{name, description, serialized}`). The browser tokenizes each
 * `serialized` string and each `description` itself; tokenization stays
 * platform-native by design.
 */
export function parse_tools_list(tools_json: string): string;

/**
 * When the vendored pricing tables were last checked (YYYY-MM). A separate
 * function rather than a wrapper around `models()` so the existing web client,
 * which parses `models()` as a bare array, keeps working unchanged.
 */
export function prices_observed(): string;

/**
 * Redact secrets in `text`. `options_json` may be empty for defaults, or a JSON
 * `RedactOptions` (`{ "strict": bool, "allow": string[] }`). Returns a JSON
 * `RedactionResult`.
 */
export function redact(text: string, options_json: string): string;

/**
 * Crate version. Proves the wasm wiring end to end.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly analyze_mcp: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly analyze_tools_inventory: (a: number, b: number, c: number) => void;
    readonly audit: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly cost: (a: number, b: number, c: number) => void;
    readonly models: (a: number) => void;
    readonly parse_tools_list: (a: number, b: number, c: number) => void;
    readonly prices_observed: (a: number) => void;
    readonly redact: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly version: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
