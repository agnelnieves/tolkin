/// <reference lib="webworker" />

// Dedicated compression worker for the LLMLingua-2 preview. The model is
// lazy-loaded on the first load/compress request, never at page load (the
// worker itself is only created when the user opts in). Model files are a
// static fetch from the Hugging Face Hub, the same sanctioned posture as the
// Gemma tokenizer; user text never leaves this worker.
//
// Protocol mirrors the tokenize worker (one response per request id, errors
// in-band) with one addition: unsolicited { kind: "progress" } messages while
// the model downloads, so the panel can render a progress bar.

import { LLMLingua2 } from "@atjsh/llmlingua-2";
import type { PreTrainedTokenizer } from "@huggingface/transformers";
import { encode } from "gpt-tokenizer/encoding/o200k_base";
import type { Tiktoken } from "js-tiktoken/lite";
import { makeLLMLingua2TokenizerAdapter } from "./tokenizer-adapter";
import type { CompressOutput } from "./types";

export type CompressWorkerRequest =
  | { id: number; op: "load" }
  | { id: number; op: "compress"; text: string; rate: number };

export type CompressWorkerResponse =
  | { kind: "reply"; id: number; ok: true; op: "load" }
  | { kind: "reply"; id: number; ok: true; op: "compress"; result: CompressOutput }
  | { kind: "reply"; id: number; ok: false; error: string }
  | { kind: "progress"; percent: number };

const ctx = self as unknown as DedicatedWorkerGlobalScope;

// TinyBERT is the smallest published LLMLingua-2 ONNX export (57.1 MB fp32
// model.onnx + ~1 MB tokenizer files; the repo ships no quantized variant).
// MobileBERT (99 MB) and the larger BERT/XLM-RoBERTa variants trade size for
// accuracy we do not need in a preview.
const MODEL_ID = "atjsh/llmlingua-2-js-tinybert-meetingbank";

// Mirrors the upstream demo's TinyBERT settings (max_seq_length 312 stays
// safely under the model's 512 position limit while keeping chunks small
// enough for wasm inference).
const LLMLINGUA2_CONFIG = { max_batch_size: 50, max_force_token: 100, max_seq_length: 312 };

// Soft input cap: TinyBERT inference on the wasm backend is roughly linear in
// chunk count, and beyond this size a single compress can take minutes.
const MAX_INPUT_CHARS = 200_000;

type Compressor = InstanceType<typeof LLMLingua2.PromptCompressor>;

let compressorPromise: Promise<Compressor> | null = null;

// transformers.js reports per-file download progress; aggregate by bytes so
// the 57 MB model.onnx dominates the percentage the way users expect.
function makeProgressForwarder() {
  const files = new Map<string, { loaded: number; total: number }>();
  let lastSent = -1;
  return (info: { status?: string; file?: string; loaded?: number; total?: number }) => {
    if (info.status !== "progress" || !info.file || !info.total) return;
    files.set(info.file, { loaded: info.loaded ?? 0, total: info.total });
    let loaded = 0;
    let total = 0;
    for (const f of files.values()) {
      loaded += f.loaded;
      total += f.total;
    }
    const percent = Math.min(99, Math.floor((loaded / total) * 100));
    if (percent !== lastSent) {
      lastSent = percent;
      ctx.postMessage({ kind: "progress", percent } satisfies CompressWorkerResponse);
    }
  };
}

async function loadCompressor(): Promise<Compressor> {
  const { AutoConfig, AutoTokenizer, BertForTokenClassification, env } = await import(
    "@huggingface/transformers"
  );
  // Browser-only: skip the local-models fallback that hits node:fs.
  env.allowLocalModels = false;
  env.allowRemoteModels = true;

  const progress_callback = makeProgressForwarder();
  const tjsConfig = { device: "auto", dtype: "fp32" } as const;

  const config = await AutoConfig.from_pretrained(MODEL_ID);
  const withConfig = { config: { ...config, "transformers.js_config": tjsConfig } };
  const tokenizer = await AutoTokenizer.from_pretrained(MODEL_ID, {
    ...withConfig,
    progress_callback,
  });
  const model = await BertForTokenClassification.from_pretrained(MODEL_ID, {
    ...withConfig,
    progress_callback,
  });

  return new LLMLingua2.PromptCompressor(
    model,
    // The adapter satisfies every member the compressor touches; the declared
    // parameter type is v4's PreTrainedTokenizer, hence the cast.
    makeLLMLingua2TokenizerAdapter(tokenizer) as unknown as PreTrainedTokenizer,
    LLMLingua2.get_pure_tokens_bert_base_multilingual_cased,
    LLMLingua2.is_begin_of_new_word_bert_base_multilingual_cased,
    // Only .encode() is ever called on this (verified against 2.0.3 dist).
    // Reuse gpt-tokenizer's o200k ranks already in the bundle instead of
    // shipping js-tiktoken's duplicate ~3 MB rank table.
    { encode: (text: string) => encode(text) } as unknown as Tiktoken,
    LLMLINGUA2_CONFIG,
    // Default logger is console.log; keep the worker quiet.
    () => {},
  );
}

function ensureCompressor(): Promise<Compressor> {
  if (!compressorPromise) {
    compressorPromise = loadCompressor().catch((e: unknown) => {
      // Reset so a retry can re-attempt the download instead of replaying the
      // cached rejection forever.
      compressorPromise = null;
      throw e;
    });
  }
  return compressorPromise;
}

async function runCompress(text: string, rate: number): Promise<CompressOutput> {
  if (text.trim().length === 0) {
    throw new Error("Nothing to compress: the input is empty.");
  }
  if (text.length > MAX_INPUT_CHARS) {
    throw new Error(
      `Input is ${text.length.toLocaleString()} characters; the compression preview is capped at ${MAX_INPUT_CHARS.toLocaleString()}. Compress a smaller section.`,
    );
  }
  if (!(rate > 0 && rate < 1)) {
    throw new Error("Compression rate must be between 0 and 1.");
  }
  const compressor = await ensureCompressor();
  const compressed = await compressor.compress(text, { rate });
  return {
    compressed,
    originalTokensApprox: encode(text).length,
    compressedTokensApprox: encode(compressed).length,
  };
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

ctx.addEventListener("message", (event: MessageEvent<CompressWorkerRequest>) => {
  const data = event.data;
  void (async () => {
    try {
      if (data.op === "load") {
        await ensureCompressor();
        ctx.postMessage({
          kind: "reply",
          id: data.id,
          ok: true,
          op: "load",
        } satisfies CompressWorkerResponse);
      } else {
        const result = await runCompress(data.text, data.rate);
        ctx.postMessage({
          kind: "reply",
          id: data.id,
          ok: true,
          op: "compress",
          result,
        } satisfies CompressWorkerResponse);
      }
    } catch (e) {
      ctx.postMessage({
        kind: "reply",
        id: data.id,
        ok: false,
        error: errorMessage(e),
      } satisfies CompressWorkerResponse);
    }
  })();
});
