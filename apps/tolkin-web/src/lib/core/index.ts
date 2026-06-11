// Public tolkin-core API. redact(), cost(), and models() run inside a single
// dedicated Web Worker (see ./worker.ts) so the WASM module loads at most once
// and the regex-heavy redaction pass never blocks the UI. The worker is created
// lazily on first use by the client, guarded against SSR.

export {
  analyzeMcp,
  analyzeToolsInventory,
  audit,
  cost,
  models,
  parseToolsList,
  pricesObserved,
  redact,
  terminate,
} from "./client";
export type {
  AuditFinding,
  AuditOptions,
  AuditReport,
  CallCost,
  CoreProvider,
  CostBreakdown,
  CostRequest,
  Finding,
  FormatPreview,
  McpAnalysis,
  McpScenarios,
  McpServerReport,
  McpSlimOption,
  McpSlimRecommendation,
  McpToolInventory,
  McpToolRow,
  McpToolSmell,
  McpToolSpec,
  McpToolTokenCount,
  McpTotals,
  ModelLongContext,
  ModelPrice,
  Recommendation,
  RedactionResult,
  RedactOptions,
} from "./types";
