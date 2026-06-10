// Gemma's SentencePiece vocab is shared by every current Gemini model
// (262,144 tokens). `@huggingface/transformers` loads the tokenizer.json from
// the Hugging Face Hub on first use and caches it via the browser's HTTP
// cache. We do not run any model inference here; this is tokenizer-only.

import type { Segment } from "./types";

type GemmaTokenizer = {
  encode(text: string, options?: { add_special_tokens?: boolean }): number[];
  // SentencePiece pieces, e.g. "▁hello". Returned by transformers.js v4's
  // PreTrainedTokenizer.tokenize(). Length matches encode() for the same
  // add_special_tokens setting.
  tokenize(text: string, options?: { add_special_tokens?: boolean }): string[];
};

// SentencePiece marks a token that begins a new whitespace-delimited word with
// U+2581 (LOWER ONE EIGHTH BLOCK, "▁"). For display we turn that marker into a
// real leading space so chips read naturally.
const SPM_SPACE = "▁";

let instance: Promise<GemmaTokenizer> | null = null;

async function loadTokenizer(): Promise<GemmaTokenizer> {
  if (instance) return instance;
  instance = (async () => {
    const { AutoTokenizer, env } = await import("@huggingface/transformers");
    // Browser-only: skip the local-models fallback that hits node:fs.
    env.allowLocalModels = false;
    env.allowRemoteModels = true;
    const tokenizer = await AutoTokenizer.from_pretrained("Xenova/gemma-tokenizer");
    return tokenizer as unknown as GemmaTokenizer;
  })();
  return instance;
}

export async function count(text: string): Promise<number> {
  if (text.length === 0) return 0;
  const tokenizer = await loadTokenizer();
  // add_special_tokens defaults to true and would inflate the count by the BOS
  // token. Real Gemini billing matches the bare encoding.
  const ids = tokenizer.encode(text, { add_special_tokens: false });
  return ids.length;
}

// One segment per token: zip the bare ids with their SentencePiece pieces.
// Both calls use add_special_tokens: false so the BOS token is excluded and the
// two arrays line up. The "▁" word-boundary marker becomes a leading space.
export async function segments(text: string): Promise<{ tokens: number; segments: Segment[] }> {
  if (text.length === 0) return { tokens: 0, segments: [] };
  const tokenizer = await loadTokenizer();
  const ids = tokenizer.encode(text, { add_special_tokens: false });
  const pieces = tokenizer.tokenize(text, { add_special_tokens: false });
  const segs: Segment[] = ids.map((id, i) => {
    const piece = pieces[i] ?? "";
    return { id, text: piece.replaceAll(SPM_SPACE, " ") };
  });
  return { tokens: ids.length, segments: segs };
}
