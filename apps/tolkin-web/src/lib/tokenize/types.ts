export type Provider = "openai" | "anthropic" | "gemini";

export type TokenCount = {
  provider: Provider;
  tokens: number;
  approximate?: boolean;
  approximateNote?: string;
};

// One visible piece per token. `text` is the human-readable slice the token
// decodes to. `id` is the vocabulary id (useful for hover inspection). For
// providers without a public vocabulary (Anthropic) no segments are produced.
export type Segment = {
  id: number;
  text: string;
};

// Segment extraction result. `exact` is false for any provider whose token
// boundaries are estimated rather than derived from a real vocabulary. When
// `exact` is false, `segments` is empty: Tolkin never fabricates per-token
// chips. `segments.length` equals `tokens` for exact providers.
export type SegmentResult = {
  provider: Provider;
  exact: boolean;
  tokens: number;
  segments: Segment[];
};
