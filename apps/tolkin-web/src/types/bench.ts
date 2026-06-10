// Schema contract for the benchmark harness output (results.json).
// The harness at apps/tolkin-cli/benchmarks/ emits this shape; the /bench
// page renders it. Bump `v` on breaking changes, never mutate silently.

export interface BenchResults {
  v: 1;
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

export interface ConfigurationCase {
  id: string;
  fixture: string;
  client_shape: string;
  tokenizer: string;
  servers: number;
  cold_tokens: number;
  swap_savings_tokens: number;
  slim_savings_tokens: number;
  pct_of_200k_window: number;
  notes?: string;
}

export interface LossyTrack {
  fidelity: "lossy";
  rct_caveat: string;
  quality_scoring: { scored: boolean; method: string };
  cases: LossyCase[];
  comparisons: ExternalComparison[];
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
