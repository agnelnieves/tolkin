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
    <section className="space-y-3 rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-medium text-zinc-300">Secret redaction</h2>
        {loading ? (
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">scanning...</span>
        ) : (
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">runs first</span>
        )}
      </div>

      <p className="text-xs text-zinc-400">
        {empty ? (
          loading ? (
            "Scanning for secrets..."
          ) : (
            "No secrets detected."
          )
        ) : (
          <>
            <span className="font-medium text-zinc-200">{redactedCount.toLocaleString()}</span>{" "}
            {redactedCount === 1 ? "secret" : "secrets"} redacted,{" "}
            <span className="font-medium text-zinc-200">{reviewCount.toLocaleString()}</span>{" "}
            flagged for review.
          </>
        )}
      </p>

      {empty ? null : (
        <ul className="space-y-1.5">
          {findings.map((f, i) => (
            <li
              key={`${f.kind}-${f.start}-${f.end}-${i}`}
              className="flex items-center justify-between gap-3 rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2"
            >
              <div className="flex min-w-0 items-center gap-2">
                <Badge redacted={f.redacted} />
                <span className="truncate font-mono text-xs text-zinc-200">{f.kind}</span>
              </div>
              <div className="flex shrink-0 items-center gap-3 text-[11px] tabular-nums text-zinc-500">
                <span>{Math.round(f.confidence * 100)}%</span>
                <span className="font-mono">
                  {f.start}..{f.end}
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Badge({ redacted }: { redacted: boolean }) {
  return redacted ? (
    <span className="rounded bg-emerald-950 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-emerald-300">
      redacted
    </span>
  ) : (
    <span className="rounded bg-amber-950 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider text-amber-300">
      review
    </span>
  );
}
