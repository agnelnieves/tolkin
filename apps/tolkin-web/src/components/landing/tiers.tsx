import { SectionHeading } from "./section-heading";

const TIERS = [
  {
    name: "identified",
    basis: "advisory estimate",
    body: "What audit, mcp, and project flag as reclaimable right now. An estimate, and labeled as one.",
  },
  {
    name: "realized",
    basis: "measured structure, estimated frequency",
    body: "The measured delta between ledger snapshots of the same project. Structural evidence of what you actually removed.",
  },
  {
    name: "measured",
    basis: "ground truth",
    body: "Actual spend from your own ingested session logs. Reconciled token-for-token before it is reported.",
  },
] as const;

export function Tiers() {
  return (
    <section className="border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Honesty tiers"
          title="Numbers that admit what they are."
          lede="Every figure Tolkin prints belongs to exactly one tier, and the tier is always printed next to it. No blending, no rounding up the confidence."
        />

        <div className="mt-14 grid grid-cols-1 gap-px overflow-hidden rounded-xl border border-white/10 bg-white/10 md:grid-cols-3">
          {TIERS.map((tier, i) => (
            <div key={tier.name} className="flex flex-col gap-4 bg-background p-8">
              <span className="font-mono text-xs text-muted-foreground tabular-nums">
                0{i + 1}
              </span>
              <h3 className="font-display text-2xl font-semibold tracking-tight">{tier.name}</h3>
              <p className="font-mono text-xs text-lime-300">&quot;{tier.basis}&quot;</p>
              <p className="text-sm leading-relaxed text-muted-foreground">{tier.body}</p>
            </div>
          ))}
        </div>

        <p className="mt-8 max-w-2xl text-sm leading-relaxed text-muted-foreground">
          Model output is a fourth class,{" "}
          <span className="font-mono text-xs text-foreground">&quot;model advisory (local)&quot;</span>,
          and it never joins the tiers.
        </p>
      </div>
    </section>
  );
}
