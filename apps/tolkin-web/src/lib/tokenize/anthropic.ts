// Anthropic has not released their tokenizer publicly. The community
// approximation that bpe-lite ships under its `anthropic` provider is just the
// OpenAI cl100k_base encoding (bpe-lite labels it "cl100k approximation,
// ~95% accurate"). bpe-lite itself is Node-only: it depends on `Buffer` from
// node:buffer and reads vocab files via node:fs, so we cannot bundle it for
// the browser without a polyfill stack. Reaching for cl100k_base directly via
// gpt-tokenizer's per-encoding entry point gets us the same approximator
// without the Node baggage, and keeps the bundle small.
//
// Surfaces MUST label this output as an estimate. Phase 2 introduces the
// opt-in hybrid "Verify with Anthropic" call for exact counts.
import { countTokens } from "gpt-tokenizer/encoding/cl100k_base";
import type { Segment } from "./types";

export async function count(text: string): Promise<number> {
  if (text.length === 0) return 0;
  return countTokens(text);
}

// Anthropic's tokenizer is closed, so the cl100k_base boundaries here do NOT
// correspond to real Claude tokens. We return the estimated count for the band
// UI but never any segments: rendering fabricated per-token chips would
// misrepresent how Claude actually splits the text. `exact` is always false.
export async function segments(text: string): Promise<{ tokens: number; segments: Segment[] }> {
  const tokens = await count(text);
  return { tokens, segments: [] };
}
