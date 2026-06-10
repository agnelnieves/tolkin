// TypeScript shapes for the tolkin-core WASM API. These mirror the Rust
// serde structs one to one (snake_case field names are preserved on purpose so
// JSON.parse output maps directly onto the type without remapping). The core is
// the single source of truth: redaction patterns, cost math, and pricing all
// live in Rust. Nothing here recomputes any of it.

export type CoreProvider = "openai" | "anthropic" | "gemini";

// One detected secret. The web surface only ever receives kind / label /
// offsets / confidence / a redacted flag. It never receives the raw matched
// text, which is the privacy guarantee: the original secret cannot be read back
// out of a Finding.
export interface Finding {
  kind: string;
  label: string;
  start: number;
  end: number;
  confidence: number;
  redacted: boolean;
}

export interface RedactionResult {
  redacted_text: string;
  findings: Finding[];
  redacted_count: number;
  review_count: number;
}

// Input to redact(). Empty / omitted means "use core defaults".
export interface RedactOptions {
  strict?: boolean;
  allow?: string[];
}

// Input to cost(). Only model_id and input_tokens are required; everything else
// falls back to a core default (see PLAN section 10).
export interface CostRequest {
  model_id: string;
  input_tokens: number;
  output_tokens?: number;
  calls?: number;
  cache_hit_rate?: number;
  cache_write_tokens?: number;
  cache_ttl?: "5m" | "1h";
  batch_fraction?: number;
  long_context?: boolean;
}

export interface CallCost {
  fresh_input: number;
  cached_input: number;
  cache_write: number;
  output: number;
  total: number;
}

export interface CostBreakdown {
  model_id: string;
  provider: CoreProvider;
  display: string;
  input_tokens: number;
  fresh_tokens: number;
  cached_tokens: number;
  output_tokens: number;
  output_estimated: boolean;
  long_context_applied: boolean;
  calls: number;
  per_call: CallCost;
  total: CallCost;
  notes: string[];
}

export interface ModelLongContext {
  threshold: number;
  input: number;
  output: number;
}

export interface ModelPrice {
  id: string;
  provider: CoreProvider;
  display: string;
  input: number;
  output: number;
  cache_read: number | null;
  cache_write_5m: number | null;
  cache_write_1h: number | null;
  long_context: ModelLongContext | null;
  is_default: boolean;
}

// Audit shapes. These mirror the Rust audit serde structs one to one. The
// audit engine runs every production-proven waste rule over the text and
// returns ranked findings; all detection and savings math lives in Rust.

// Input to audit(). input_tokens carries a real token count when the caller
// already tokenized the text; omitted / null falls back to the core's bytes/4
// approximation, which the report labels in its notes. include_experimental
// opts in to the experimental rules (higher false-positive risk); their
// findings carry the "experimental" badge and the report gains a warning note.
export interface AuditOptions {
  input_tokens?: number | null;
  include_experimental?: boolean;
}

// One converted-format preview attached to a finding. The core only emits this
// when a rule can show the exact transformation; the preview text may be capped
// by the core to keep reports small, but bytes_after always reflects the full
// conversion.
export interface FormatPreview {
  kind: "json-minify" | "json-toon" | "html-markdown";
  preview: string;
  bytes_before: number;
  bytes_after: number;
  fidelity: "lossless" | "near-lossless" | "lossy-low-risk";
  caveat: string;
}

// One detected source of token waste. Named AuditFinding (not Finding) because
// the redactor already exports a Finding with a different shape.
export interface AuditFinding {
  rule: string;
  severity: string;
  title: string;
  detail: string;
  byte_start: number;
  byte_end: number;
  savings_min: number;
  savings_max: number;
  confidence: number;
  badge: string;
  citation: string;
  // Converted-format preview; absent when the rule has none to show.
  preview?: FormatPreview;
}

export interface AuditReport {
  findings: AuditFinding[];
  total_savings_min: number;
  total_savings_max: number;
  notes: string[];
}

// MCP analyzer shapes. These mirror the Rust McpAnalysis serde structs one to
// one. analyze_mcp() parses any agent's MCP config (Claude Desktop, Claude Code,
// Cursor, Continue, VS Code / Copilot, Zed), matches each server against the
// built-in catalog, and reports the token cost of its tool definitions plus
// which servers to swap for an official CLI. All catalog lookups and math live
// in Rust; nothing here recomputes any of it.

// What the core recommends doing with a server. "replace" swaps it for a CLI,
// "replace-for-ad-hoc" keeps it but suggests a CLI for occasional use, "keep"
// leaves it as is, "unknown" means the server is not in the catalog.
export type Recommendation = "replace" | "replace-for-ad-hoc" | "keep" | "unknown";

// Token cost of a server's tool definitions under four loading strategies.
export interface McpScenarios {
  cold: number;
  warm: number;
  tool_search: number;
  capacity: number;
}

// One verified way to register fewer tools for a server without removing it.
// The snippet is the exact JSON fragment or args change to paste; location says
// where it goes ("env" | "args" | "url").
export interface McpSlimOption {
  mechanism: string;
  snippet: string;
  location: string;
  est_reduction_pct: number;
  note: string;
  source_hint: string;
}

// Per-server slim recommendation. already_slimmed means the config already
// carries the slim env var / flag / url param, so the cold estimate was scaled
// down by the core and est_tokens_saved is 0.
export interface McpSlimRecommendation {
  already_slimmed: boolean;
  option: McpSlimOption;
  est_tokens_saved: number;
}

export interface McpServerReport {
  name: string;
  matched_id: string | null;
  display: string;
  transport: string;
  tools: number | null;
  cold_tokens: number | null;
  scenarios: McpScenarios | null;
  cli_alternative: string | null;
  recommendation: Recommendation;
  note: string;
  savings_tokens: number;
  savings_usd: number;
  // Slim recommendation; null when the server has no verified slim mechanism.
  // Independent of the CLI-swap savings: per server the two are alternatives.
  slim: McpSlimRecommendation | null;
}

export interface McpTotals {
  servers: number;
  matched: number;
  unknown: number;
  cold: number;
  warm: number;
  tool_search: number;
  capacity: number;
  pct_of_window: number;
  savings_tokens: number;
  savings_usd: number;
  cold_usd: number;
  // Estimated savings from slimming servers you keep (registering fewer
  // tools). Reported separately from the CLI-swap savings; per server the two
  // are alternative levers and must not be summed.
  slim_savings_tokens: number;
  slim_savings_usd: number;
}

export interface McpAnalysis {
  client: string;
  provider: CoreProvider;
  servers: McpServerReport[];
  totals: McpTotals;
  notes: string[];
}
