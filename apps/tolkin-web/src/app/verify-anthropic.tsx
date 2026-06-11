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
    <div className="rounded-lg border border-white/10 bg-black/30 p-4">
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
          Anthropic
        </h3>
        <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/70 tabular-nums">
          {text.length.toLocaleString()} chars
        </span>
      </div>

      <p className="mt-3 font-display text-3xl font-semibold tabular-nums text-foreground">
        {state.error && !verified ? (
          <span className="text-base font-normal text-destructive">error</span>
        ) : verified ? (
          <>
            <span className="text-lime-300">{verified.result.inputTokens.toLocaleString()}</span>
            <span className="ml-2 text-sm font-normal text-muted-foreground">tokens</span>
          </>
        ) : state.value === null && state.loading ? (
          <span className="text-base font-normal text-muted-foreground">loading</span>
        ) : state.value === null ? (
          <span className="text-base font-normal text-muted-foreground">no input</span>
        ) : (
          <>
            <span className="text-muted-foreground">~ </span>
            <span className="text-lime-300">{state.value.toLocaleString()}</span>
            <span className="ml-2 text-sm font-normal text-muted-foreground">tokens</span>
          </>
        )}
      </p>

      {verified ? (
        <>
          <p className="mt-1 text-xs text-lime-300">exact (verified), {verified.result.model}</p>
          {verified.localEstimate !== null && verified.result.inputTokens > 0 ? (
            <p className="mt-1 text-xs text-muted-foreground">
              local estimate was ~{verified.localEstimate.toLocaleString()} (
              {deltaLabel(verified.localEstimate, verified.result.inputTokens)})
            </p>
          ) : null}
        </>
      ) : (
        <p className="mt-1 text-xs text-muted-foreground">
          {state.error ?? "estimate, cl100k_base, +/- 10%"}
        </p>
      )}

      {expanded ? (
        <div className="mt-3 space-y-2 border-t border-white/10 pt-3">
          <p className="text-[11px] leading-4 text-muted-foreground">
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
            className="block w-full rounded-md border border-white/10 bg-black/40 px-3 py-2 font-mono text-base text-foreground placeholder:text-muted-foreground/60 focus:border-lime-300/60 focus:outline-none sm:text-xs"
          />
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onConfirm}
              disabled={pending || apiKey.trim().length === 0 || text.length === 0}
              className="inline-flex min-h-11 items-center justify-center rounded-md border border-lime-300/40 bg-lime-300/10 px-3 py-2 text-xs text-lime-200 transition-colors duration-150 ease-out hover:bg-lime-300/15 focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {pending ? "verifying" : "Verify"}
            </button>
            <button
              type="button"
              onClick={() => setExpanded(false)}
              className="text-xs text-muted-foreground hover:text-foreground"
            >
              cancel
            </button>
          </div>
          {failure ? <p className="text-xs text-destructive">{failure}</p> : null}
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setExpanded(true)}
          className="mt-3 inline-flex min-h-11 items-center rounded-md border border-white/10 bg-white/[0.02] px-3 py-2 text-xs text-muted-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
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
