"use client";

import gsap from "gsap";
import { useEffect, useRef } from "react";

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

// Stats band: four huge tabular numerals that count up once when the band
// first enters the viewport. Server HTML carries the final values, so the
// numbers are correct without JavaScript and under reduced motion.
export function StatsBand() {
  const root = useRef<HTMLDivElement>(null);
  const played = useRef(false);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    const tweens: gsap.core.Tween[] = [];
    const observer = new IntersectionObserver(
      (entries) => {
        if (played.current) return;
        if (!entries.some((entry) => entry.isIntersecting)) return;
        played.current = true;
        observer.disconnect();

        const targets = el.querySelectorAll<HTMLElement>("[data-stat-value]");
        targets.forEach((target, i) => {
          const stat = STATS[i];
          if (!stat) return;
          const step = 1 / 10 ** stat.decimals;
          const counter = { v: 0 };
          tweens.push(
            gsap.to(counter, {
              v: stat.value,
              duration: 1.1,
              ease: "power2.out",
              snap: { v: step },
              onUpdate: () => {
                target.textContent = format(stat, counter.v);
              },
            }),
          );
        });
      },
      { threshold: 0.35 },
    );

    observer.observe(el);
    return () => {
      observer.disconnect();
      for (const tween of tweens) tween.kill();
    };
  }, []);

  return (
    <section aria-label="Measured numbers" className="border-b border-white/10">
      <div
        ref={root}
        className="mx-auto grid max-w-[1200px] grid-cols-1 gap-y-10 px-6 py-14 sm:grid-cols-2 sm:py-20 lg:grid-cols-4"
      >
        {STATS.map((stat) => (
          <div
            key={stat.label}
            className="flex flex-col gap-3 lg:border-l lg:border-white/10 lg:px-8 lg:first:border-l-0 lg:first:pl-0"
          >
            <span
              data-stat-value
              className="font-display text-5xl font-semibold tracking-tight text-lime-300 tabular-nums sm:text-6xl"
            >
              {format(stat, stat.value)}
            </span>
            <span className="max-w-[26ch] font-mono text-xs leading-relaxed text-muted-foreground">
              {stat.label}
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
