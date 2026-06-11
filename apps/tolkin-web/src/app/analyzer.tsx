"use client";

import { useEffect, useRef, useState } from "react";
import { ANALYZER_EXAMPLES } from "@/components/analyzer/examples";
import { PanelCard, PanelHeader } from "@/components/analyzer/panel";
import type { Finding } from "../lib/core";
import { redact } from "../lib/core";
import { AuditPanel } from "./audit-panel";
import { CompressPanel } from "./compress-panel";
import { CoreVersion } from "./core-version";
import { CostPanel } from "./cost-panel";
import { FileDrop } from "./file-drop";
import { RedactionLedger } from "./redaction-ledger";
import { TokenizerPanel } from "./tokenizer-panel";
import { Visualizer } from "./visualizer";

type RedactionState = {
  // the text the redactor ran on, so we know whether the result is still current
  source: string;
  redactedText: string;
  findings: Finding[];
  redactedCount: number;
  reviewCount: number;
  loading: boolean;
};

const initialRedaction: RedactionState = {
  source: "",
  redactedText: "",
  findings: [],
  redactedCount: 0,
  reviewCount: 0,
  loading: false,
};

// Client wrapper that owns the single source of truth for the input text. The
// textarea, the file dropzone, the redaction ledger, the count panel, the
// visualizer, and the cost panel all read from this one piece of state.
//
// Redaction runs first: when `redactOn` is true, every downstream view analyzes
// the redacted text, never the raw paste. While the redactor is still working on
// the current text we fall back to the raw text so the UI never blocks; once the
// fresh result arrives the views switch to it.
export function Analyzer() {
  const [text, setText] = useState("");
  const [redactOn, setRedactOn] = useState(true);
  const [redaction, setRedaction] = useState<RedactionState>(initialRedaction);
  const runRef = useRef(0);

  // Debounced redaction effect. Mirrors the tokenizer panel: 100ms debounce,
  // stale-run cancellation via a run-id ref.
  useEffect(() => {
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setRedaction((s) => ({ ...s, loading: true }));
      redact(text).then(
        (r) => {
          if (runRef.current !== runId) return;
          setRedaction({
            source: text,
            redactedText: r.redacted_text,
            findings: r.findings,
            redactedCount: r.redacted_count,
            reviewCount: r.review_count,
            loading: false,
          });
        },
        () => {
          if (runRef.current !== runId) return;
          // On a redaction failure, fall back to passing the raw text through
          // (effectiveText already does this) and clear the ledger.
          setRedaction({ ...initialRedaction, source: text });
        },
      );
    }, 100);
    return () => clearTimeout(handle);
  }, [text]);

  // The result is only usable when it was computed for the text on screen.
  const redactionFresh = redaction.source === text && !redaction.loading;
  const effectiveText = redactOn && redactionFresh ? redaction.redactedText : text;
  const hasText = text.length > 0;

  return (
    <section className="w-full" aria-label="Token analyzer">
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12 lg:gap-8">
        {/* Left column: input + tokenizer + visualizer (the instrument). */}
        <div className="space-y-6 lg:col-span-7">
          <PanelCard className="p-0 sm:p-0">
            <div className="p-4 sm:p-5">
              <PanelHeader
                title="Input"
                blurb="Paste text or drop a file. Common formats are parsed in your browser."
                status={hasText ? `${text.length.toLocaleString()} chars` : undefined}
                action={
                  <label className="inline-flex cursor-pointer items-center gap-2 rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-xs text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground">
                    <input
                      type="checkbox"
                      checked={redactOn}
                      onChange={(e) => setRedactOn(e.target.checked)}
                      className="h-3.5 w-3.5 accent-lime-300"
                      aria-label="Redact secrets before analysis"
                    />
                    <span>Redact secrets before analysis</span>
                  </label>
                }
              />

              <label className="block">
                <span className="sr-only">Paste text to tokenize</span>
                <textarea
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  placeholder="Paste a prompt, a config, a doc. Nothing leaves your browser."
                  rows={hasText ? 10 : 6}
                  spellCheck={false}
                  className="block w-full resize-y rounded-lg border border-white/10 bg-black/40 px-4 py-3 font-mono text-base leading-6 text-foreground placeholder:text-muted-foreground/60 focus:border-lime-300/60 focus:outline-none focus:ring-2 focus:ring-lime-300/40 sm:text-sm"
                />
              </label>

              <div className="mt-4 flex flex-col gap-3">
                <FileDrop onText={setText} compact={hasText} />

                {!hasText ? (
                  <div className="space-y-2">
                    <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
                      Try a sample
                    </p>
                    <div className="flex flex-wrap gap-2">
                      {ANALYZER_EXAMPLES.map((ex) => (
                        <button
                          key={ex.id}
                          type="button"
                          onClick={() => setText(ex.text)}
                          title={ex.blurb}
                          className="inline-flex min-h-11 items-center gap-2 rounded-full border border-white/10 bg-white/[0.03] px-4 py-2 font-mono text-xs text-muted-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
                        >
                          {ex.label}
                        </button>
                      ))}
                    </div>
                  </div>
                ) : null}

                {redactOn && redaction.redactedCount > 0 ? (
                  <p
                    role="status"
                    className="rounded-md border border-lime-300/30 bg-lime-300/5 px-3 py-2 font-mono text-[11px] text-lime-200"
                  >
                    {redaction.redactedCount.toLocaleString()}{" "}
                    {redaction.redactedCount === 1 ? "secret" : "secrets"} redacted before analysis.
                  </p>
                ) : null}
              </div>
            </div>
          </PanelCard>

          <TokenizerPanel text={effectiveText} />

          <Visualizer text={effectiveText} />
        </div>

        {/* Right column: stacked result panels. */}
        <div className="space-y-6 lg:col-span-5">
          <RedactionLedger
            findings={redaction.findings}
            redactedCount={redaction.redactedCount}
            reviewCount={redaction.reviewCount}
            loading={redaction.loading}
          />

          <CostPanel text={effectiveText} />

          <AuditPanel text={effectiveText} />

          <CompressPanel text={effectiveText} />

          <p className="px-1 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground/70">
            v0.1.0 scaffolding. <CoreVersion />
          </p>
        </div>
      </div>
    </section>
  );
}
