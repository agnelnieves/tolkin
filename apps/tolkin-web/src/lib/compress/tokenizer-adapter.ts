// Compatibility adapter between @huggingface/transformers v4 tokenizers and
// @atjsh/llmlingua-2 2.0.3, which was written against the v3 tokenizer shape.
//
// Why this exists: the library's peer range is "*" but its PromptCompressor
// reaches into three v3-only surfaces that v4 removed when tokenization moved
// to the native `tokenizers` core:
//   - tokenizer.decoder.decode(tokens)        (string join, WordPiece rules)
//   - tokenizer.model.convert_ids_to_tokens() (id -> token-string lookup)
//   - tokenizer.special_tokens                (iterable of special tokens)
// The compressor takes its tokenizer by constructor injection, so we hand it
// this adapter built only from v4 public APIs instead of patching the library.
// Verified against the 2.0.3 dist source: these are the only members touched.

import type { PreTrainedTokenizer } from "@huggingface/transformers";

type BatchEncodeOptions = { padding: boolean; truncation: boolean };

export type LLMLingua2TokenizerAdapter = {
  (texts: string[], options: BatchEncodeOptions): Promise<unknown>;
  tokenize(text: string): string[];
  special_tokens: string[];
  decoder: { decode(tokens: string[]): string };
  model: { convert_ids_to_tokens(ids: Array<number | bigint>): string[] };
};

// v3 WordPieceDecoder.decode_chain + clean_up_tokenization, reproduced as a
// pure string operation. The compressor only ever calls decoder.decode() with
// arrays of token strings (or already-merged words), never ids: subsequent
// "##" continuation pieces are glued on, everything else joins with a space,
// then the standard English punctuation cleanup runs.
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
  // Invert the vocab once for id -> token lookups. v4 returns a Map here;
  // v3 returned a plain object (handling both keeps the adapter honest if the
  // return type shifts again).
  const vocab = tokenizer.get_vocab() as Map<string, number> | Record<string, number>;
  const entries: Iterable<[string, number]> = vocab instanceof Map ? vocab : Object.entries(vocab);
  const idToToken = new Map<number, string>();
  for (const [token, id] of entries) idToToken.set(Number(id), token);

  // The compressor calls the tokenizer itself for batch encoding, so the
  // adapter is a function with the other members attached as properties.
  const callable = tokenizer as unknown as (
    texts: string[],
    options: BatchEncodeOptions,
  ) => Promise<unknown>;
  const adapter = ((texts: string[], options: BatchEncodeOptions) =>
    callable(texts, options)) as LLMLingua2TokenizerAdapter;

  adapter.tokenize = (text: string) => tokenizer.tokenize(text);
  // PromptCompressor runs Object.entries() over this and collects the values,
  // which works for arrays (index keys) exactly as it did for v3's map shape.
  adapter.special_tokens = tokenizer.all_special_tokens;
  adapter.decoder = { decode: wordPieceJoin };
  adapter.model = {
    convert_ids_to_tokens: (ids) => ids.map((id) => idToToken.get(Number(id)) ?? "[UNK]"),
  };
  return adapter;
}
