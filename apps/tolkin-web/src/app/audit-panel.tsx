"use client";

import { useEffect, useRef, useState } from "react";
import type { AuditFinding, AuditReport, FormatPreview } from "../lib/core";
import { audit } from "../lib/core";
import { count } from "../lib/tokenize";

// Token-waste audit. Lighthouse-style ranked findings over the same (already
// redacted) text the other panels analyze. Every detection, savings range, and
// note comes from the WASM core via audit(); nothing here recomputes any of it.
// The panel tokenizes the text once (OpenAI o200k, via the existing tokenize
// client) and hands that count to the core so the figures are exact; if the
// count fails the core falls back to its bytes/4 approximation and labels it
// in the report notes.

type AuditState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ok"; report: AuditReport }
  | { status: "error"; message: string };

export function AuditPanel({ text }: { text: string }) {
  const [state, setState] = useState<AuditState>({ status: "idle" });
  const [experimental, setExperimental] = useState(false);
  const runRef = useRef(0);

  // Debounced audit. Mirrors the other panels: run-id guard so a slow earlier
  // run cannot overwrite a newer result. Empty text skips the core call.
  // Toggling the experimental rules re-runs the audit through the same path.
  useEffect(() => {
    if (text === "") {
      runRef.current++;
      setState({ status: "idle" });
      return;
    }
    const handle = setTimeout(() => {
      const runId = ++runRef.current;
      setState({ status: "loading" });
      void (async () => {
        let report: AuditReport;
        try {
          // A real token count makes the core's figures exact and drops the
          // bytes/4 note. If tokenization fails, audit without it: the core
          // falls back and labels the approximation itself.
          let inputTokens: number | undefined;
          try {
            inputTokens = (await count("openai", text)).tokens;
          } catch {
            inputTokens = undefined;
          }
          report = await audit(text, {
            ...(inputTokens === undefined ? {} : { input_tokens: inputTokens }),
            include_experimental: experimental,
          });
        } catch (e) {
          if (runRef.current === runId) setState({ status: "error", message: errorMessage(e) });
          return;
        }
        if (runRef.current === runId) setState({ status: "ok", report });
      })();
    }, 300);
    return () => clearTimeout(handle);
  }, [text, experimental]);

  const report = state.status === "ok" ? state.report : null;

  return (
    <section className="w-full space-y-4">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h2 className="text-sm font-medium text-zinc-300">Audit</h2>
        <div className="flex flex-wrap items-baseline gap-3">
          {report && report.findings.length > 0 ? (
            <span className="text-xs tabular-nums text-zinc-400">
              ~{report.total_savings_min.toLocaleString()} to{" "}
              {report.total_savings_max.toLocaleString()} input tokens reclaimable
            </span>
          ) : null}
          <label
            className="flex cursor-pointer items-center gap-1.5 text-xs text-zinc-400"
            title="Higher false-positive risk; review before acting."
          >
            <input
              type="checkbox"
              checked={experimental}
              onChange={(e) => setExperimental(e.target.checked)}
              className="h-3.5 w-3.5 accent-violet-500"
            />
            Experimental rules
          </label>
        </div>
      </div>

      {state.status === "idle" ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
          Paste text above to audit it for token waste.
        </p>
      ) : state.status === "loading" ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
          auditing...
        </p>
      ) : state.status === "error" ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-sm text-red-400">
          {state.message}
        </p>
      ) : state.report.findings.length === 0 ? (
        <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
          No waste detected by the {experimental ? "enabled" : "production-proven"} rules.
        </p>
      ) : (
        <div className="space-y-2">
          {state.report.findings.map((f) => (
            <FindingRow key={`${f.rule}-${f.byte_start}`} finding={f} />
          ))}
        </div>
      )}

      {report && report.notes.length > 0 ? (
        <div className="space-y-1 border-t border-zinc-800 pt-3 text-xs leading-5 text-zinc-500">
          {report.notes.map((n) => (
            <p key={n}>{n}</p>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function FindingRow({ finding }: { finding: AuditFinding }) {
  const isExperimental = finding.badge === "experimental";
  return (
    <details className="group rounded-lg border border-zinc-800 bg-zinc-900/40">
      <summary className="flex cursor-pointer flex-wrap items-center gap-2 px-4 py-3 text-xs [&::-webkit-details-marker]:hidden">
        <SeverityBadge severity={finding.severity} />
        {isExperimental ? (
          <span className="rounded bg-violet-950 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-violet-300">
            experimental
          </span>
        ) : null}
        <span className="font-mono text-zinc-500">{finding.rule}</span>
        <span className="font-medium text-zinc-200">{finding.title}</span>
        <span className="ml-auto tabular-nums text-emerald-300">
          {formatSavings(finding.savings_min, finding.savings_max)}
        </span>
      </summary>
      <div className="space-y-2 border-t border-zinc-800 px-4 py-3">
        <p className="text-xs leading-5 text-zinc-400">{finding.detail}</p>
        <div className="flex flex-wrap items-center gap-3 text-[11px] text-zinc-500">
          <span className="tabular-nums">{Math.round(finding.confidence * 100)}% confidence</span>
          {isExperimental ? null : (
            <span className="rounded bg-zinc-800 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-400">
              {finding.badge}
            </span>
          )}
          <a
            href={finding.citation}
            target="_blank"
            rel="noreferrer"
            className="text-zinc-500 underline decoration-zinc-700 underline-offset-2 hover:text-zinc-300"
          >
            citation
          </a>
        </div>
        {finding.preview ? <PreviewBlock preview={finding.preview} /> : null}
      </div>
    </details>
  );
}

// Converted-format preview for a finding. Everything shown here (the converted
// text, byte counts, fidelity, caveat) comes from the core; this block only
// renders it and offers a clipboard copy (memory only, not storage).
function PreviewBlock({ preview }: { preview: FormatPreview }) {
  const [copied, setCopied] = useState(false);
  const fidelityStyles: Record<string, string> = {
    lossless: "bg-emerald-950 text-emerald-300",
    "near-lossless": "bg-zinc-800 text-zinc-400",
    "lossy-low-risk": "bg-amber-950 text-amber-300",
  };
  return (
    <div className="space-y-2 rounded-md border border-zinc-800 bg-zinc-950/60 p-3">
      <div className="flex flex-wrap items-center gap-3 text-[11px] text-zinc-500">
        <span className="font-medium uppercase tracking-wider text-zinc-400">Preview</span>
        <span
          className={`rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider ${fidelityStyles[preview.fidelity] ?? fidelityStyles["near-lossless"]}`}
        >
          {preview.fidelity}
        </span>
        <span className="tabular-nums">
          {preview.bytes_before.toLocaleString()} to {preview.bytes_after.toLocaleString()} bytes
        </span>
        <button
          type="button"
          className="ml-auto rounded border border-zinc-700 px-2 py-0.5 text-[11px] text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"
          onClick={() => {
            void navigator.clipboard.writeText(preview.preview).then(() => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            });
          }}
        >
          {copied ? "copied" : "copy"}
        </button>
      </div>
      {preview.caveat !== "" ? (
        <p className="text-[11px] leading-4 text-zinc-500">{preview.caveat}</p>
      ) : null}
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-zinc-900/60 p-2 font-mono text-[11px] leading-4 text-zinc-300">
        {preview.preview}
      </pre>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: string }) {
  // The core emits "high" | "medium" | "low"; anything new falls back to the
  // low style rather than crashing on an unknown key.
  const styles: Record<string, string> = {
    high: "bg-red-950 text-red-300",
    medium: "bg-amber-950 text-amber-300",
    low: "bg-zinc-800 text-zinc-400",
  };
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider ${styles[severity] ?? styles.low}`}
    >
      {severity}
    </span>
  );
}

function formatSavings(min: number, max: number): string {
  if (min === max) return `saves ~${max.toLocaleString()} tokens`;
  return `saves ~${min.toLocaleString()}-${max.toLocaleString()} tokens`;
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
