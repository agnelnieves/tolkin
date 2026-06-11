"use client";

import { useEffect, useRef, useState } from "react";
import { EmptyHint, PanelCard, PanelHeader } from "@/components/analyzer/panel";
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
    <PanelCard>
      <PanelHeader
        title="Audit"
        blurb="Rules that flag wasted tokens, with an estimated saving for each."
        action={
          <label
            className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground"
            title="Higher false-positive risk; review before acting."
          >
            <input
              type="checkbox"
              checked={experimental}
              onChange={(e) => setExperimental(e.target.checked)}
              className="h-3.5 w-3.5 accent-lime-300"
            />
            Experimental rules
          </label>
        }
        status={
          report && report.findings.length > 0
            ? `~${report.total_savings_min.toLocaleString()} to ${report.total_savings_max.toLocaleString()} tokens reclaimable`
            : undefined
        }
      />

      {state.status === "idle" ? (
        <EmptyHint>Paste text to audit it for token waste.</EmptyHint>
      ) : state.status === "loading" ? (
        <EmptyHint>auditing</EmptyHint>
      ) : state.status === "error" ? (
        <p className="rounded-lg border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
          {state.message}
        </p>
      ) : state.report.findings.length === 0 ? (
        <EmptyHint>
          No waste detected by the {experimental ? "enabled" : "production-proven"} rules.
        </EmptyHint>
      ) : (
        <div className="space-y-2">
          {state.report.findings.map((f) => (
            <FindingRow key={`${f.rule}-${f.byte_start}`} finding={f} />
          ))}
        </div>
      )}

      {report && report.notes.length > 0 ? (
        <div className="mt-3 space-y-1 border-t border-white/10 pt-3 text-xs leading-5 text-muted-foreground">
          {report.notes.map((n) => (
            <p key={n}>{n}</p>
          ))}
        </div>
      ) : null}
    </PanelCard>
  );
}

function FindingRow({ finding }: { finding: AuditFinding }) {
  const isExperimental = finding.badge === "experimental";
  return (
    <details className="group rounded-lg border border-white/10 bg-black/30 transition-colors duration-150 ease-out hover:[@media(hover:hover)]:border-white/20">
      <summary className="flex cursor-pointer flex-wrap items-center gap-2 px-4 py-3 text-xs [&::-webkit-details-marker]:hidden">
        <SeverityBadge severity={finding.severity} />
        {isExperimental ? (
          <span className="rounded border border-violet-400/30 bg-violet-400/10 px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] text-violet-300">
            experimental
          </span>
        ) : null}
        <span className="font-mono text-muted-foreground">{finding.rule}</span>
        <span className="font-medium text-foreground">{finding.title}</span>
        <span className="ml-auto font-mono tabular-nums text-lime-300">
          {formatSavings(finding.savings_min, finding.savings_max)}
        </span>
      </summary>
      <div className="space-y-2 border-t border-white/10 px-4 py-3">
        <p className="text-xs leading-5 text-muted-foreground">{finding.detail}</p>
        <div className="flex flex-wrap items-center gap-3 font-mono text-[11px] text-muted-foreground">
          <span className="tabular-nums">{Math.round(finding.confidence * 100)}% confidence</span>
          {isExperimental ? null : (
            <span className="rounded border border-white/10 bg-white/[0.03] px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
              {finding.badge}
            </span>
          )}
          <a
            href={finding.citation}
            target="_blank"
            rel="noreferrer"
            className="text-muted-foreground underline decoration-white/20 underline-offset-2 hover:text-foreground hover:decoration-lime-300"
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
    lossless: "border-lime-300/30 bg-lime-300/10 text-lime-200",
    "near-lossless": "border-white/15 bg-white/[0.03] text-muted-foreground",
    "lossy-low-risk": "border-amber-400/30 bg-amber-400/10 text-amber-300",
  };
  return (
    <div className="space-y-2 rounded-md border border-white/10 bg-black/40 p-3">
      <div className="flex flex-wrap items-center gap-3 font-mono text-[11px] text-muted-foreground">
        <span className="font-medium uppercase tracking-[0.18em] text-foreground">Preview</span>
        <span
          className={`rounded border px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.18em] ${fidelityStyles[preview.fidelity] ?? fidelityStyles["near-lossless"]}`}
        >
          {preview.fidelity}
        </span>
        <span className="tabular-nums">
          {preview.bytes_before.toLocaleString()} to {preview.bytes_after.toLocaleString()} bytes
        </span>
        <button
          type="button"
          className="ml-auto rounded-md border border-white/10 bg-white/[0.03] px-2 py-1 text-[11px] text-muted-foreground transition-colors duration-150 ease-out hover:border-lime-300/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-lime-300/60"
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
        <p className="text-[11px] leading-4 text-muted-foreground">{preview.caveat}</p>
      ) : null}
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-black/60 p-2 font-mono text-[11px] leading-4 text-foreground/80">
        {preview.preview}
      </pre>
    </div>
  );
}

function SeverityBadge({ severity }: { severity: string }) {
  // The core emits "high" | "medium" | "low"; anything new falls back to the
  // low style rather than crashing on an unknown key.
  const styles: Record<string, string> = {
    high: "border-destructive/40 bg-destructive/15 text-destructive",
    medium: "border-amber-400/30 bg-amber-400/10 text-amber-300",
    low: "border-white/15 bg-white/[0.03] text-muted-foreground",
  };
  return (
    <span
      className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] ${styles[severity] ?? styles.low}`}
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
