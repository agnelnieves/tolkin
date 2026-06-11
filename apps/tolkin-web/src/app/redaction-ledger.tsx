import { PanelCard, PanelHeader } from "@/components/analyzer/panel";
import type { Finding } from "../lib/core";

// Presentational ledger of redaction findings. This component is privacy-load-
// bearing: it renders only the metadata of each finding (kind, confidence, byte
// offsets, and whether it was auto-redacted). It never receives or renders the
// original secret text, and nothing here is copyable or DOM-readable as the raw
// value. A Finding by construction carries no secret bytes, so there is nothing
// sensitive to leak even by accident.

type Props = {
  findings: Finding[];
  redactedCount: number;
  reviewCount: number;
  loading: boolean;
};

export function RedactionLedger({ findings, redactedCount, reviewCount, loading }: Props) {
  const empty = findings.length === 0;

  return (
    <PanelCard>
      <PanelHeader
        title="Secret redaction"
        blurb="API keys, tokens, and credentials are scanned and masked before any other panel sees your text."
        status={loading ? "scanning" : "runs first"}
      />

      <p className="text-sm text-muted-foreground">
        {empty ? (
          loading ? (
            "Scanning for secrets."
          ) : (
            "No secrets detected."
          )
        ) : (
          <>
            <span className="font-medium text-foreground">{redactedCount.toLocaleString()}</span>{" "}
            {redactedCount === 1 ? "secret" : "secrets"} redacted,{" "}
            <span className="font-medium text-foreground">{reviewCount.toLocaleString()}</span>{" "}
            flagged for review.
          </>
        )}
      </p>

      {empty ? null : (
        <ul className="mt-3 space-y-1.5">
          {findings.map((f, i) => (
            <li
              key={`${f.kind}-${f.start}-${f.end}-${i}`}
              className="flex items-center justify-between gap-3 rounded-md border border-white/10 bg-black/30 px-3 py-2"
            >
              <div className="flex min-w-0 items-center gap-2">
                <Badge redacted={f.redacted} />
                <span className="truncate font-mono text-xs text-foreground">{f.kind}</span>
              </div>
              <div className="flex shrink-0 items-center gap-3 font-mono text-[11px] tabular-nums text-muted-foreground">
                <span>{Math.round(f.confidence * 100)}%</span>
                <span>
                  {f.start}..{f.end}
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </PanelCard>
  );
}

function Badge({ redacted }: { redacted: boolean }) {
  return redacted ? (
    <span className="rounded border border-lime-300/30 bg-lime-300/10 px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] text-lime-200">
      redacted
    </span>
  ) : (
    <span className="rounded border border-amber-400/30 bg-amber-400/10 px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-[0.18em] text-amber-300">
      review
    </span>
  );
}
