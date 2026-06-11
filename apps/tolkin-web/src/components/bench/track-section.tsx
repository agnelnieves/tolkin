import type { ReactNode } from "react";

type FidelityKind = "lossless" | "lossless-configuration" | "lossy";

const FIDELITY_STYLES: Record<FidelityKind, string> = {
  lossless: "border-emerald-400/30 text-emerald-300",
  "lossless-configuration": "border-sky-400/30 text-sky-300",
  lossy: "border-amber-400/30 text-amber-300",
};

const FIDELITY_LABELS: Record<FidelityKind, string> = {
  lossless: "lossless",
  "lossless-configuration": "lossless-config",
  lossy: "lossy",
};

export function FidelityBadge({ fidelity }: { fidelity: string }) {
  const style =
    FIDELITY_STYLES[fidelity as FidelityKind] ?? "border-white/10 text-zinc-400";
  const label = FIDELITY_LABELS[fidelity as FidelityKind] ?? fidelity;
  return (
    <span
      className={`inline-flex items-center rounded border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider ${style}`}
    >
      {label}
    </span>
  );
}

export function HonestyLabel({
  status,
  reason,
}: {
  status: "measured" | "not-runnable-headless" | "not-comparable";
  reason?: string;
}) {
  if (status === "measured") {
    return (
      <span className="inline-flex items-center rounded border border-lime-400/40 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-lime-300">
        measured
      </span>
    );
  }
  const label = status === "not-runnable-headless" ? "not-runnable" : "not-comparable";
  return (
    <span
      className="inline-flex items-center rounded border border-white/10 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-zinc-500"
      title={reason}
    >
      {label}
    </span>
  );
}

export function PendingNote() {
  return (
    <p className="rounded-lg border border-white/10 bg-white/[0.02] p-4 font-mono text-xs text-zinc-500">
      Results pending. Run the harness at apps/tolkin-cli/benchmarks to populate this track.
    </p>
  );
}

// Responsive table wrapper: horizontal scroll on desktop, stacked cards on narrow viewports
export function TrackTable({
  head,
  body,
}: {
  head: ReactNode;
  body: ReactNode;
}) {
  return (
    <div className="overflow-x-auto rounded-lg border border-white/10">
      <table className="w-full min-w-[600px] border-collapse text-left text-xs">
        <thead className="border-b border-white/10 bg-white/[0.03]">
          <tr className="text-[10px] uppercase tracking-wider text-zinc-500">{head}</tr>
        </thead>
        <tbody>{body}</tbody>
      </table>
    </div>
  );
}

export function Th({ children, right }: { children: ReactNode; right?: boolean }) {
  return (
    <th className={`px-3 py-2.5 font-medium${right ? " text-right" : ""}`}>{children}</th>
  );
}

export function Td({
  children,
  mono,
  right,
  dim,
  accent,
}: {
  children: ReactNode;
  mono?: boolean;
  right?: boolean;
  dim?: boolean;
  accent?: boolean;
}) {
  let cls = "px-3 py-2.5";
  if (right) cls += " text-right";
  if (mono) cls += " font-mono tabular-nums";
  if (dim) cls += " text-zinc-400";
  else if (accent) cls += " text-lime-300";
  else cls += " text-zinc-200";
  return <td className={cls}>{children}</td>;
}

export function TrackEyebrow({ label }: { label: string }) {
  return (
    <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">{label}</p>
  );
}

export function SectionDivider() {
  return <hr className="border-white/[0.06]" />;
}
