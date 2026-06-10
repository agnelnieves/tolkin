// Per-encoding subpath import keeps the bundle to one encoding's worth of merges
// (~50 KB min+gz). Use o200k_base, the encoding shared by GPT-4o, the o-series,
// GPT-4.1, and GPT-5.
import { countTokens, decode, encode } from "gpt-tokenizer/encoding/o200k_base";
import type { Segment } from "./types";

// Replacement glyph (U+25AF, white vertical rectangle) for a token whose bytes
// are only part of a multi-byte UTF-8 sequence and therefore cannot stand alone
// as printable text. Decoding a single such id yields the Unicode replacement
// character (U+FFFD); we surface our own marker so the visualizer can show that
// the token is a byte fragment rather than a real glyph.
const PARTIAL_BYTE_GLYPH = "▯";
const REPLACEMENT_CHAR = "�";

export function count(text: string): number {
  if (text.length === 0) return 0;
  return countTokens(text);
}

// One segment per token. `decode([id])` round-trips a single token to its text.
// A token that holds a partial multi-byte sequence decodes to the replacement
// character; we render PARTIAL_BYTE_GLYPH for those so the chip count still
// matches the token count exactly (segments.length === ids.length).
export function segments(text: string): { tokens: number; segments: Segment[] } {
  if (text.length === 0) return { tokens: 0, segments: [] };
  const ids = encode(text);
  const segs: Segment[] = ids.map((id) => {
    const piece = decode([id]);
    // A lone partial-byte token decodes to either the replacement character or
    // an empty string; render a visible marker for both so no chip is invisible.
    const fragment = piece === REPLACEMENT_CHAR || piece === "";
    return { id, text: fragment ? PARTIAL_BYTE_GLYPH : piece };
  });
  return { tokens: ids.length, segments: segs };
}
