"use client";

import { useRef, useState } from "react";
import type { VerifyResult } from "../lib/verify/anthropic";
import { verifyWithAnthropic } from "../lib/verify/anthropic";

type CountState = {
  value: number | null;
  loading: boolean;
  error: string | null;
};

// Each verify outcome remembers the exact text it ran on. The result is only
// trusted while the panel text still matches; re-typing falls back to the
// estimate. The SHA-keyed cache in lib/verify makes re-verifying the same
// text free.
type VerifyState =
  | { status: "idle" }
  | { status: "pending"; forText: string }
  | { status: "done"; forText: string; result: VerifyResult; localEstimate: number | null }
  | { status: "error"; forText: string; message: string };

// The Anthropic provider card with the opt-in BYOK "Verify with Anthropic"
// flow. The `text` prop is the analyzer's effective text, which has already
// been through the redactor; only that redacted text ever leaves the browser.
// The API key lives in component state only: no browser storage of any kind,
// no cookies, no context provider.
export function AnthropicCard({ text, state }: { text: string; state: CountState }) {
  const [verify, setVerify] = useState<VerifyState>({ status: "idle" });
  const [expanded, setExpanded] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const runRef = useRef(0);

  const verified = verify.status === "done" && verify.forText === text ? verify : null;
  const pending = verify.status === "pending" && verify.forText === text;
  const failure = verify.status === "error" && verify.forText === text ? verify.message : null;

  async function onConfirm() {
    const runId = ++runRef.current;
    const forText = text;
    const localEstimate = state.value;
    setVerify({ status: "pending", forText });
    try {
      const result = await verifyWithAnthropic(forText, apiKey.trim());
      if (runRef.current !== runId) return;
      setVerify({ status: "done", forText, result, localEstimate });
      setExpanded(false);
    } catch (e) {
      if (runRef.current !== runId) return;
      setVerify({
        status: "error",
        forText,
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium text-zinc-300">Anthropic</h2>
        <span className="text-[10px] uppercase tracking-wider text-zinc-500">
          {text.length.toLocaleString()} chars
        </span>
      </div>

      <p className="mt-3 text-3xl font-semibold tabular-nums text-zinc-100">
        {state.error && !verified ? (
          <span className="text-base font-normal text-red-400">error</span>
        ) : verified ? (
          <>
            {verified.result.inputTokens.toLocaleString()}
            <span className="ml-2 text-sm font-normal text-zinc-500">tokens</span>
          </>
        ) : state.value === null && state.loading ? (
          <span className="text-base font-normal text-zinc-500">loading...</span>
        ) : state.value === null ? (
          <span className="text-base font-normal text-zinc-500">-</span>
        ) : (
          <>
            <span className="text-zinc-500">~ </span>
            {state.value.toLocaleString()}
            <span className="ml-2 text-sm font-normal text-zinc-500">tokens</span>
          </>
        )}
      </p>

      {verified ? (
        <>
          <p className="mt-1 text-xs text-emerald-400">exact (verified), {verified.result.model}</p>
          {verified.localEstimate !== null && verified.result.inputTokens > 0 ? (
            <p className="mt-1 text-xs text-zinc-500">
              local estimate was ~{verified.localEstimate.toLocaleString()} (
              {deltaLabel(verified.localEstimate, verified.result.inputTokens)})
            </p>
          ) : null}
        </>
      ) : (
        <p className="mt-1 text-xs text-zinc-500">
          {state.error ?? "estimate, cl100k_base, +/- 10%"}
        </p>
      )}

      {expanded ? (
        <div className="mt-3 space-y-2 border-t border-zinc-800 pt-3">
          <p className="text-[11px] leading-4 text-zinc-500">
            Sends the redacted text to api.anthropic.com using your key. The key stays in memory and
            is never stored.
          </p>
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-ant-..."
            aria-label="Anthropic API key"
            className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1.5 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-zinc-600 focus:outline-none"
          />
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onConfirm}
              disabled={pending || apiKey.trim().length === 0 || text.length === 0}
              className="rounded-md border border-emerald-800 bg-emerald-950/40 px-2.5 py-1 text-xs text-emerald-300 hover:bg-emerald-950/70 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {pending ? "verifying..." : "Verify"}
            </button>
            <button
              type="button"
              onClick={() => setExpanded(false)}
              className="text-xs text-zinc-500 hover:text-zinc-300"
            >
              cancel
            </button>
          </div>
          {failure ? <p className="text-xs text-red-400">{failure}</p> : null}
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setExpanded(true)}
          className="mt-3 rounded-md border border-zinc-800 px-2.5 py-1 text-xs text-zinc-400 hover:border-zinc-700 hover:text-zinc-200"
        >
          {verified ? "Re-verify with Anthropic" : "Verify with Anthropic"}
        </button>
      )}
    </div>
  );
}

// Signed percent of how far the local estimate landed from the exact count.
function deltaLabel(estimate: number, exact: number): string {
  const pct = ((estimate - exact) / exact) * 100;
  const sign = pct >= 0 ? "+" : "";
  return `${sign}${pct.toFixed(1)}% off`;
}
