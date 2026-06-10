// SANCTIONED NETWORK EGRESS. This module is the ONLY place in the web app
// allowed to call fetch() with user content. It implements the opt-in, BYOK
// "Verify with Anthropic" hybrid: a user-initiated call to the free
// /v1/messages/count_tokens endpoint that turns the labeled Claude estimate
// into an exact count.
//
// - It sends ONLY redacted text (the caller passes the panel text, which has
//   already been through the redactor).
// - It runs only when the user explicitly confirms, with their own API key.
// - The key is passed in per call and never stored here; the in-memory cache
//   below keys on a content hash and holds token counts only.
// - The documented privacy scan (the grep for network and storage APIs)
//   allowlists src/lib/verify/. Keep every fetch in this file so the
//   allowlist stays narrow.

export type VerifyResult = {
  inputTokens: number;
  model: string;
  fromCache: boolean;
};

const ENDPOINT = "https://api.anthropic.com/v1/messages/count_tokens";
const DEFAULT_MODEL = "claude-sonnet-4-6";

// In-memory only. Keyed by SHA-256 of `${model}\n${text}` so iterative tuning
// pays the network round trip once per unique input. Never persisted.
const cache = new Map<string, number>();

async function cacheKey(model: string, text: string): Promise<string> {
  const data = new TextEncoder().encode(`${model}\n${text}`);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

function httpError(status: number): Error {
  if (status === 401) return new Error("invalid API key");
  if (status === 429) return new Error("rate limited, try again shortly");
  return new Error(`count_tokens request failed (HTTP ${status})`);
}

export async function verifyWithAnthropic(
  text: string,
  apiKey: string,
  model?: string,
): Promise<VerifyResult> {
  const resolvedModel = model ?? DEFAULT_MODEL;
  const key = await cacheKey(resolvedModel, text);
  const cached = cache.get(key);
  if (cached !== undefined) {
    return { inputTokens: cached, model: resolvedModel, fromCache: true };
  }

  const res = await fetch(ENDPOINT, {
    method: "POST",
    headers: {
      "x-api-key": apiKey,
      "anthropic-version": "2023-06-01",
      "content-type": "application/json",
      // Documented header that opts in to CORS for direct browser calls.
      "anthropic-dangerous-direct-browser-access": "true",
    },
    body: JSON.stringify({
      model: resolvedModel,
      messages: [{ role: "user", content: text }],
    }),
  });

  if (!res.ok) throw httpError(res.status);

  const data: unknown = await res.json();
  const inputTokens =
    typeof data === "object" && data !== null && "input_tokens" in data
      ? (data as { input_tokens: unknown }).input_tokens
      : undefined;
  if (typeof inputTokens !== "number") {
    throw new Error("unexpected count_tokens response shape");
  }

  cache.set(key, inputTokens);
  return { inputTokens, model: resolvedModel, fromCache: false };
}
