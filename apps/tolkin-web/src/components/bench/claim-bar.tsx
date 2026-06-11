"use client";

import { useEffect, useRef, useState } from "react";

export interface ClaimRow {
  tool: string;
  claimed_pct: number | null;
  claimed_label: string;
  measured_pct: number;
  condition: string;
  status: "measured" | "not-comparable" | "not-runnable";
  reason?: string;
}

function Bar({
  pct,
  variant,
  animate,
}: {
  pct: number;
  variant: "claimed" | "measured";
  animate: boolean;
}) {
  const isClaimed = variant === "claimed";
  const width = Math.max(0, Math.min(100, pct));

  return (
    <div className="relative h-5 w-full overflow-hidden rounded-sm bg-white/[0.04]">
      <div
        className="absolute inset-y-0 left-0 origin-left rounded-sm transition-transform"
        style={{
          width: `${width}%`,
          backgroundColor: isClaimed ? "rgba(255,255,255,0.18)" : "rgb(163,230,53)",
          transform: animate ? "scaleX(1)" : "scaleX(0)",
          transitionDuration: animate ? (isClaimed ? "700ms" : "900ms") : "0ms",
          transitionDelay: animate ? (isClaimed ? "0ms" : "120ms") : "0ms",
          transitionTimingFunction: "cubic-bezier(0.16,1,0.3,1)",
        }}
      />
    </div>
  );
}

function HonestyBadge({ status, reason }: { status: ClaimRow["status"]; reason?: string }) {
  const base =
    "inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider";
  if (status === "measured") {
    return (
      <span className={`${base} border border-lime-400/40 text-lime-300`} aria-label="measured">
        measured
      </span>
    );
  }
  const label = status === "not-runnable" ? "not-runnable" : "not-comparable";
  return (
    <span
      className={`${base} border border-white/10 text-zinc-500`}
      title={reason}
      aria-label={`${label}: ${reason ?? ""}`}
    >
      {label}
    </span>
  );
}

export function ClaimBarRow({ row }: { row: ClaimRow }) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    // Respect prefers-reduced-motion at the JS level too
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    if (mq.matches) {
      setVisible(true);
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setVisible(true);
            observer.disconnect();
          }
        }
      },
      { threshold: 0.2 },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const delta = row.claimed_pct !== null ? (row.claimed_pct - row.measured_pct).toFixed(1) : null;

  return (
    <div ref={ref} className="group/row space-y-2 py-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-sm font-medium text-zinc-100">{row.tool}</span>
        <HonestyBadge status={row.status} reason={row.reason} />
        {delta !== null && Number(delta) > 0 && (
          <span className="font-mono text-xs text-amber-400/80" aria-label={`gap: ${delta}%`}>
            +{delta}% claimed vs measured
          </span>
        )}
      </div>

      <div className="grid grid-cols-[6.5rem_1fr_4.5rem] items-center gap-3">
        <span className="font-mono text-[11px] text-zinc-500 tabular-nums">claimed</span>
        {row.claimed_pct !== null ? (
          <Bar pct={row.claimed_pct} variant="claimed" animate={visible} />
        ) : (
          <div className="h-5 rounded-sm bg-white/[0.04]">
            <span className="pl-2 font-mono text-[11px] text-zinc-600">no public figure</span>
          </div>
        )}
        <span className="text-right font-mono text-sm font-semibold tabular-nums text-zinc-400">
          {row.claimed_pct !== null ? `${row.claimed_pct.toFixed(1)}%` : "N/A"}
        </span>
      </div>

      <div className="grid grid-cols-[6.5rem_1fr_4.5rem] items-center gap-3">
        <span className="font-mono text-[11px] text-zinc-500 tabular-nums">measured</span>
        <Bar pct={row.measured_pct} variant="measured" animate={visible} />
        <span className="text-right font-mono text-lg font-semibold tabular-nums text-lime-300">
          {row.measured_pct.toFixed(2)}%
        </span>
      </div>

      <p className="text-[12px] leading-relaxed text-zinc-500">{row.condition}</p>
    </div>
  );
}

export function ClaimBarSection({ rows }: { rows: ClaimRow[] }) {
  return (
    <div className="divide-y divide-white/[0.06]">
      {rows.map((row) => (
        <ClaimBarRow key={row.tool} row={row} />
      ))}
    </div>
  );
}
