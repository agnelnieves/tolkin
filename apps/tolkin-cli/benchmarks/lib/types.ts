// Local mirror of apps/tolkin-web/src/types/bench.ts. The harness emits this
// shape; the /bench page renders it. The bench.ts schema in tolkin-web is the
// single source of truth; this type only exists so the runner has a local
// `type` annotation without crossing the workspace boundary.
//
// Keep in lockstep with bench.ts. Any drift surfaces immediately when
// `bun run --filter=tolkin-web typecheck` reads bench-results.json against
// the schema.

export interface BenchResults {
  v: 2;
  generated_at: string;
  tolkin_version: string;
  prices_observed: string;
  runs: number;
  environment: BenchEnvironment;
  methodology_md: string;
  tracks: {
    structural: StructuralTrack;
    configuration: ConfigurationTrack;
    lossy: LossyTrack;
  };
}

export interface BenchEnvironment {
  runner: string;
  os: string;
  tokenizers: Record<string, string>;
}

export interface StructuralTrack {
  fidelity: "lossless";
  cases: StructuralCase[];
}

export interface StructuralCase {
  id: string;
  fixture: string;
  technique: string;
  tokenizer: string;
  before_tokens: number;
  after_tokens: number;
  savings_tokens: number;
  savings_pct: number;
  injection_overhead_tokens: number;
  variance: { runs: number; min: number; max: number };
  notes?: string;
}

export interface ConfigurationTrack {
  fidelity: "lossless-configuration";
  cases: ConfigurationCase[];
  comparisons: ExternalComparison[];
}

// Provenance of a vendored tools/list manifest (mirrors bench.ts).
export interface ManifestProvenance {
  source: string;
  license: string;
  captured: string;
  server_version: string;
}

export interface ConfigurationCase {
  id: string;
  fixture: string;
  client_shape: string;
  tokenizer: string;
  // v2: every row declares its basis; current rows are all
  // tokenized-manifest. cold_tokens is the exact tokenized weight of the
  // tool definitions (no provider price multipliers).
  basis: "tokenized-manifest" | "catalog-estimate";
  servers: number;
  tools: number;
  cold_tokens: number;
  swap_savings_tokens: number;
  slim_savings_tokens: number;
  pct_of_200k_window: number;
  manifest: ManifestProvenance;
  notes?: string;
}

export interface LossyTrack {
  fidelity: "lossy";
  rct_caveat: string;
  quality_scoring: LossyQualityScoring;
  cases: LossyCase[];
  comparisons: ExternalComparison[];
}

// Track-level scoring metadata. See bench.ts for full doc.
export interface LossyQualityScoring {
  scored: boolean;
  method: string;
  grader_model?: string;
  anthropic_version?: string;
}

export interface LossyScoredAnswer {
  question_id: string;
  question: string;
  reply: string;
  accepted_answers: string[];
  correct: boolean;
}

export interface LossyScoredCase {
  case_id: string;
  questions_total: number;
  questions_correct: number;
  accuracy: number;
  answers: LossyScoredAnswer[];
}

export interface LossyCase {
  id: string;
  fixture: string;
  technique: string;
  tokenizer: string;
  target_ratio: number;
  before_tokens: number;
  after_tokens: number;
  achieved_ratio: number;
  savings_pct: number;
  notes?: string;
  scored?: LossyScoredCase;
}

export interface ExternalComparison {
  name: string;
  status: "measured" | "not-runnable-headless" | "not-comparable";
  reason: string;
  before_tokens?: number;
  after_tokens?: number;
  savings_pct?: number;
  notes?: string;
}
