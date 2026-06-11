import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

// PanelCard is the analyzer's shadcn-style card. Hairline border, near-black
// fill from the landing theme, consistent internal rhythm. All analyzer panels
// (tokenizer, visualizer, cost, audit, mcp, compression, ledger) wear this
// shell so the page reads as one instrument.
export function PanelCard({
  children,
  className,
  as: As = "section",
  ...rest
}: {
  children: ReactNode;
  className?: string;
  as?: "section" | "div";
} & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <As
      className={cn("rounded-xl border border-white/10 bg-white/[0.015] p-4 sm:p-5", className)}
      {...rest}
    >
      {children}
    </As>
  );
}

// PanelHeader: mono uppercase 11px letterspaced eyebrow, plus the one-line
// plain-language explainer for non-technical visitors. Optional trailing
// action slot for a switch, badge, or select that belongs in the header row.
export function PanelHeader({
  title,
  blurb,
  action,
  status,
}: {
  title: string;
  blurb?: ReactNode;
  action?: ReactNode;
  status?: ReactNode;
}) {
  return (
    <header className="mb-4 flex flex-col gap-3 sm:mb-5 sm:flex-row sm:items-start sm:justify-between">
      <div className="min-w-0 space-y-1.5">
        <div className="flex items-center gap-3">
          <h2 className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
            {title}
          </h2>
          {status ? (
            <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground/70">
              {status}
            </span>
          ) : null}
        </div>
        {blurb ? (
          <p className="max-w-prose text-[13px] leading-relaxed text-muted-foreground">{blurb}</p>
        ) : null}
      </div>
      {action ? <div className="shrink-0">{action}</div> : null}
    </header>
  );
}

// Quiet mono hint used as an empty state. Reads as a label, not blank space.
export function EmptyHint({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed border-white/10 bg-black/30 px-4 py-5 text-center">
      <p className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground/80">
        {children}
      </p>
    </div>
  );
}

// Single-row mono shimmer used while a panel is waiting. We allow at most one
// shimmer in view at a time per the design brief. The pulse is opacity only
// and is gated behind motion-safe so reduced-motion users get a static state.
export function Shimmer({ className, children }: { className?: string; children?: ReactNode }) {
  return (
    <div
      aria-busy="true"
      className={cn(
        "rounded-md border border-white/10 bg-white/[0.02] px-3 py-2 motion-safe:animate-pulse",
        className,
      )}
    >
      <span className="block font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground/70">
        {children}
      </span>
    </div>
  );
}

// Tiny privacy-promise lockup used in the top bar and elsewhere: a lime dot
// plus the line. Reads as a status pill, not a banner.
export function PrivacyTag({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-2 font-mono text-[11px] tracking-[0.04em] text-muted-foreground",
        className,
      )}
    >
      <span aria-hidden="true" className="size-1.5 rounded-full bg-lime-300/90" />
      Nothing leaves your browser.
    </span>
  );
}
