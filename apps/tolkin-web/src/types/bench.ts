// Schema contract for the benchmark harness output (results.json).
// The harness at apps/tolkin-cli/benchmarks/ emits this shape; the /bench
// page renders it. Bump `v` on breaking changes, never mutate silently.
//
// v2 (2026-06): the configuration track moved from catalog estimates to
// tokenized manifests. ConfigurationCase gained `basis`, `tools`, and
// `manifest` (provenance); `cold_tokens` semantics changed from the
// analyzer's provider-surcharged cold scenario to the exact tokenized
// weight of the tool definitions (no price multipliers); the old
// config-shape cases were retired.

export interface BenchResults {
  v: 2;
  // The ONLY field allowed to differ between two consecutive runs on the
  // same tree (the determinism contract).
  generated_at: string;
  tolkin_version: string;
  prices_observed: string;
  runs: number;
  environment: BenchEnvironment;
  // Full methodology text, markdown. Single source of truth shared by
  // RESULTS.md and the /bench page.
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

// Provenance of a vendored tools/list manifest. Full detail lives in
// benchmarks/fixtures/configuration/manifests/README.md; these fields make
// results.json self-describing.
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
  // What kind of number cold_tokens is. v2 rows are all tokenized-manifest;
  // the catalog-estimate value exists so a future mixed table cannot ship
  // an unlabeled blend.
  basis: "tokenized-manifest" | "catalog-estimate";
  servers: number;
  tools: number;
  // Exact tokenized weight of the server's tool definitions (sum of
  // per-tool canonical serializations, o200k_base). No provider price
  // multipliers; cache surcharge scenarios live in the analyzer, not here.
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

// Track-level scoring metadata. When `scored: false`, only `method` is
// populated (e.g. "BYOK extraction-QA harness, off by default"). When
// `scored: true`, the grader_model and the Anthropic API version are
// recorded so a scored row's basis is fully self-describing.
export interface LossyQualityScoring {
  scored: boolean;
  method: string;
  // Present only when scored=true.
  grader_model?: string;
  anthropic_version?: string;
}

// Per-question scored answer recorded when --score-quality runs. The model's
// reply and the author's accepted answers ship together so the grading is
// reproducible (anyone running the harness can re-verify the substring
// match without rerunning the API call).
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
  // Present only when --score-quality ran for this case.
  scored?: LossyScoredCase;
}

export interface ExternalComparison {
  name: string;
  status: "measured" | "not-runnable-headless" | "not-comparable";
  reason: string;
  // Present only when status is "measured".
  before_tokens?: number;
  after_tokens?: number;
  savings_pct?: number;
  notes?: string;
}
