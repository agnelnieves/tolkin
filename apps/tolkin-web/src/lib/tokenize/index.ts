// Public tokenization API. Both count() and segments() now run inside a single
// dedicated Web Worker (see ./worker.ts) so large inputs do not block the UI and
// the Gemma vocab loads at most once. The worker is created lazily on first use
// by the client, guarded against SSR. Callers keep the same count() signature
// they had before the worker existed.

import { count as clientCount, segments as clientSegments } from "./client";
import type { Provider, SegmentResult, TokenCount } from "./types";

export type { Provider, Segment, SegmentResult, TokenCount } from "./types";

export function count(provider: Provider, text: string): Promise<TokenCount> {
  return clientCount(provider, text);
}

export function segments(provider: Provider, text: string): Promise<SegmentResult> {
  return clientSegments(provider, text);
}
