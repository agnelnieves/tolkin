// Public types for the LLMLingua-2 compression preview.

export type CompressOutput = {
  compressed: string;
  // o200k_base counts computed in the worker with gpt-tokenizer. Named
  // "approx" in the wire contract because the panel re-counts both sides via
  // the shared tokenize client for display; these are the same encoding.
  originalTokensApprox: number;
  compressedTokensApprox: number;
};

export type ModelStatus = "idle" | "downloading" | "ready" | "error";

export type ModelState = {
  status: ModelStatus;
  // 0-100, only meaningful while downloading.
  progress: number;
  error: string | null;
};
