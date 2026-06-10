// Public API for the opt-in LLMLingua-2 compression preview. Everything runs
// in a dedicated Web Worker created lazily on first use; the ~58 MB TinyBERT
// model downloads from the Hugging Face Hub only when the user asks for it.

export { compress, loadModel, modelState, subscribeModelState, terminate } from "./client";
export type { CompressOutput, ModelState, ModelStatus } from "./types";
