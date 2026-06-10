"use client";

import { useEffect, useRef, useState } from "react";
import type { Finding } from "../lib/core";
import { redact } from "../lib/core";
import { AuditPanel } from "./audit-panel";
import { CompressPanel } from "./compress-panel";
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

  return (
    <section className="w-full max-w-4xl mx-auto space-y-8">
      <div className="space-y-4">
        <label className="block">
          <span className="sr-only">Paste text to tokenize</span>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Paste a prompt, a config, a doc. Nothing leaves your browser."
            rows={12}
            spellCheck={false}
            className="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-4 py-3 font-mono text-sm leading-6 text-zinc-100 placeholder:text-zinc-600 focus:border-zinc-600 focus:outline-none focus:ring-2 focus:ring-zinc-700 resize-y"
          />
        </label>

        <div className="flex flex-wrap items-center justify-between gap-3">
          <FileDrop onText={setText} />
          <label className="flex items-center gap-2 text-xs text-zinc-400">
            <input
              type="checkbox"
              checked={redactOn}
              onChange={(e) => setRedactOn(e.target.checked)}
              className="h-3.5 w-3.5 accent-emerald-500"
            />
            <span>Redact secrets before analysis</span>
          </label>
        </div>

        {redactOn && redaction.redactedCount > 0 ? (
          <p className="rounded-md border border-emerald-900/60 bg-emerald-950/30 px-3 py-2 text-xs text-emerald-300">
            {redaction.redactedCount.toLocaleString()}{" "}
            {redaction.redactedCount === 1 ? "secret" : "secrets"} redacted before analysis.
          </p>
        ) : null}
      </div>

      <RedactionLedger
        findings={redaction.findings}
        redactedCount={redaction.redactedCount}
        reviewCount={redaction.reviewCount}
        loading={redaction.loading}
      />

      <TokenizerPanel text={effectiveText} />

      <Visualizer text={effectiveText} />

      <CostPanel text={effectiveText} />

      <AuditPanel text={effectiveText} />

      <CompressPanel text={effectiveText} />
    </section>
  );
}
