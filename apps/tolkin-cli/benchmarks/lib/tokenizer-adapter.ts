// Copy of apps/tolkin-web/src/lib/compress/tokenizer-adapter.ts (verified
// equivalence as of 2026-06). Kept here to avoid crossing the workspace
// boundary while still using the same v3-shaped adapter that the web preview
// uses against @huggingface/transformers v4. See the source file for
// background; the rules in `wordPieceJoin` are reproduced verbatim.

import type { PreTrainedTokenizer } from "@huggingface/transformers";

type BatchEncodeOptions = { padding: boolean; truncation: boolean };

export type LLMLingua2TokenizerAdapter = {
  (texts: string[], options: BatchEncodeOptions): Promise<unknown>;
  tokenize(text: string): string[];
  special_tokens: string[];
  decoder: { decode(tokens: string[]): string };
  model: { convert_ids_to_tokens(ids: Array<number | bigint>): string[] };
};

function wordPieceJoin(tokens: string[]): string {
  return tokens
    .map((token, i) => {
      if (i === 0) return token;
      return token.startsWith("##") ? token.slice(2) : ` ${token}`;
    })
    .join("")
    .replace(/ \./g, ".")
    .replace(/ \?/g, "?")
    .replace(/ !/g, "!")
    .replace(/ ,/g, ",")
    .replace(/ ' /g, "' ")
    .replace(/ n't/g, "n't")
    .replace(/ 'm/g, "'m")
    .replace(/ 's/g, "'s")
    .replace(/ 've/g, "'ve")
    .replace(/ 're/g, "'re");
}

export function makeLLMLingua2TokenizerAdapter(
  tokenizer: PreTrainedTokenizer,
): LLMLingua2TokenizerAdapter {
  const vocab = tokenizer.get_vocab() as Map<string, number> | Record<string, number>;
  const entries: Iterable<[string, number]> = vocab instanceof Map ? vocab : Object.entries(vocab);
  const idToToken = new Map<number, string>();
  for (const [token, id] of entries) idToToken.set(Number(id), token);

  const callable = tokenizer as unknown as (
    texts: string[],
    options: BatchEncodeOptions,
  ) => Promise<unknown>;
  const adapter = ((texts: string[], options: BatchEncodeOptions) =>
    callable(texts, options)) as LLMLingua2TokenizerAdapter;

  adapter.tokenize = (text: string) => tokenizer.tokenize(text);
  adapter.special_tokens = tokenizer.all_special_tokens;
  adapter.decoder = { decode: wordPieceJoin };
  adapter.model = {
    convert_ids_to_tokens: (ids) => ids.map((id) => idToToken.get(Number(id)) ?? "[UNK]"),
  };
  return adapter;
}
