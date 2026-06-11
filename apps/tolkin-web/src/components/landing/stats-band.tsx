"use client";

import { useLayoutEffect, useRef } from "react";
import { Hairline } from "./hairline";
import { gsap, MOTION_OK } from "./motion";

type Stat = {
  value: number;
  decimals: number;
  prefix?: string;
  label: string;
};

// Every figure here is measured, not projected. Sources: the cache analysis
// and TTL counterfactual on real session logs, the I2 reconciliation, and
// the MCP catalog.
const STATS: Stat[] = [
  {
    value: 0.97522,
    decimals: 5,
    label: "measured cache hit rate, on real session logs",
  },
  {
    value: 8.19,
    decimals: 2,
    prefix: "$",
    label: "won by the 1h TTL strategy in counterfactual replay",
  },
  {
    value: 169,
    decimals: 0,
    label: "sessions reconciled token-for-token",
  },
  {
    value: 22,
    decimals: 0,
    label: "MCP servers in the catalog, plus exact probe for yours",
  },
];

function format(stat: Stat, n: number): string {
  return `${stat.prefix ?? ""}${n.toFixed(stat.decimals)}`;
}

// Stats band: four huge tabular numerals. On first viewport entry each
// numeral clips up out of an overflow-hidden mask (yPercent 100 to 0,
// staggered), and the count-up starts while its line is still landing, so
// the band reads as one choreographed move. Server HTML carries the final
// values, so the numbers are correct without JavaScript and under reduced
// motion. Plays once.
export function StatsBand() {
  const root = useRef<HTMLElement>(null);

  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;

    const ctx = gsap.context(() => {
      const mm = gsap.matchMedia();

      mm.add(MOTION_OK, () => {
        const values = gsap.utils.toArray<HTMLElement>("[data-stat-value]", el);
        if (values.length === 0) return;

        const tl = gsap.timeline({
          scrollTrigger: {
            trigger: el,
            start: "top 80%",
            once: true,
            toggleActions: "play none none none",
          },
        });

        tl.from(values, {
          yPercent: 100,
          duration: 0.5,
          ease: "power3.out",
          stagger: 0.06,
        });

        values.forEach((target, i) => {
          const stat = STATS[i];
          if (!stat) return;
          const step = 1 / 10 ** stat.decimals;
          const counter = { v: 0 };
          tl.to(
            counter,
            {
              v: stat.value,
              duration: 1.1,
              ease: "power2.out",
              snap: { v: step },
              onUpdate: () => {
                target.textContent = format(stat, counter.v);
              },
            },
            0.2 + i * 0.06,
          );
        });

        return () => {
          // If the media condition flips mid-count, land on the real values.
          values.forEach((target, i) => {
            const stat = STATS[i];
            if (stat) target.textContent = format(stat, stat.value);
          });
        };
      });
    }, root);

    return () => ctx.revert();
  }, []);

  return (
    <section ref={root} aria-label="Measured numbers" className="relative">
      <div className="mx-auto grid max-w-[1200px] grid-cols-1 gap-y-10 px-6 py-14 sm:grid-cols-2 sm:py-20 lg:grid-cols-4">
        {STATS.map((stat) => (
          <div
            key={stat.label}
            className="flex flex-col gap-3 lg:border-l lg:border-white/10 lg:px-8 lg:first:border-l-0 lg:first:pl-0"
          >
            <span className="block overflow-hidden">
              <span
                data-stat-value
                className="block font-display text-5xl font-semibold tracking-tight text-lime-300 tabular-nums sm:text-6xl"
              >
                {format(stat, stat.value)}
              </span>
            </span>
            <span className="max-w-[26ch] font-mono text-xs leading-relaxed text-muted-foreground">
              {stat.label}
            </span>
          </div>
        ))}
      </div>
      <Hairline />
    </section>
  );
}
