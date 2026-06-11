import Link from "next/link";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { SectionHeading } from "./section-heading";

const ROWS = [
  {
    tool: "cavemem",
    claimed: "about 75% for prose",
    measured: "11.72%",
  },
  {
    tool: "repomix --compress",
    claimed: "about 70%",
    measured: "45.47%",
  },
] as const;

export function Verification() {
  return (
    <section className="border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Claimed vs measured"
          title="We measure vendor claims too."
          lede="Compression claims are often measured under different conditions than yours, and the conditions are rarely published. Tolkin runs the tools on a declared corpus, publishes the conditions, and reports what came out."
        />

        <div className="mt-14 overflow-hidden rounded-xl border border-white/10">
          <table className="w-full text-left">
            <thead>
              <tr className="border-b border-white/10 font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                <th scope="col" className="px-5 py-3 font-medium sm:px-6">
                  Tool
                </th>
                <th scope="col" className="px-5 py-3 font-medium sm:px-6">
                  Claimed
                </th>
                <th scope="col" className="px-5 py-3 font-medium sm:px-6">
                  Measured
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-white/10">
              {ROWS.map((row) => (
                <tr key={row.tool}>
                  <td className="px-5 py-4 font-mono text-[13px] text-foreground sm:px-6">
                    {row.tool}
                  </td>
                  <td className="px-5 py-4 font-mono text-[13px] text-muted-foreground tabular-nums sm:px-6">
                    {row.claimed}
                  </td>
                  <td className="px-5 py-4 font-mono text-[13px] text-lime-300 tabular-nums sm:px-6">
                    {row.measured}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <div className="mt-8 flex flex-wrap items-center gap-4">
          <Link href="/bench" className={cn(buttonVariants({ variant: "outline", size: "lg" }), "px-4")}>
            The full benchmark, methodology and all
          </Link>
          <p className="font-mono text-xs text-muted-foreground">
            declared fixtures, vendored licenses, reproducible runs
          </p>
        </div>
      </div>
    </section>
  );
}
