"use client";

import gsap from "gsap";
import Link from "next/link";
import { useLayoutEffect, useRef } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import { CopyButton } from "./copy-button";
import { Hairline } from "./hairline";
import { RevealText } from "./reveal-text";
import { Terminal } from "./terminal";

const INTRO_KEY = "tolkin-hero-intro";

const INSTALLS = [
  { id: "brew", label: "brew", command: "brew install agnelnieves/tolkin/tolkin" },
  { id: "npm", label: "npm", command: "npm install -g tolkin-cli" },
  { id: "bunx", label: "bunx", command: "bunx tolkin" },
] as const;

// Hero: one GSAP intro timeline, played once per session (sessionStorage),
// opacity and small translateY only, ~600ms total. Reduced motion skips it
// via gsap.matchMedia. The terminal starts typing after the intro window.
export function Hero() {
  const scope = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const mm = gsap.matchMedia(scope);

    mm.add("(prefers-reduced-motion: no-preference)", () => {
      if (sessionStorage.getItem(INTRO_KEY)) return;
      sessionStorage.setItem(INTRO_KEY, "1");
      gsap.from("[data-hero-stagger]", {
        opacity: 0,
        y: 14,
        duration: 0.4,
        ease: "power2.out",
        stagger: 0.05,
      });
    });

    return () => mm.revert();
  }, []);

  return (
    <section ref={scope} className="landing-grid relative">
      <div className="mx-auto max-w-[1200px] px-6 pt-20 pb-16 sm:pt-28 sm:pb-24">
        <p
          data-hero-stagger
          className="font-mono text-[11px] uppercase tracking-[0.24em] text-lime-300/90"
        >
          Privacy-first token analyzer for AI agent setups
        </p>

        <RevealText
          as="h1"
          className="mt-6 max-w-[14ch] font-display text-[clamp(2.75rem,8vw,7rem)] leading-[0.98] font-bold tracking-tight text-balance"
        >
          Know exactly what your agents are eating.
        </RevealText>

        <div className="mt-12 grid gap-12 lg:grid-cols-2 lg:gap-16">
          <div className="flex flex-col gap-8">
            <p
              data-hero-stagger
              className="max-w-xl text-base leading-relaxed text-muted-foreground sm:text-lg"
            >
              Tolkin measures the standing context of your AI setup: MCP servers, instruction files,
              skills, cache health. Deterministic numbers, zero network egress, receipts for every
              claim.
            </p>

            <div data-hero-stagger className="flex flex-col gap-5">
              <Tabs defaultValue="brew">
                <TabsList variant="line" className="h-9">
                  {INSTALLS.map((install) => (
                    <TabsTrigger
                      key={install.id}
                      value={install.id}
                      className="px-3 font-mono text-xs"
                    >
                      {install.label}
                    </TabsTrigger>
                  ))}
                </TabsList>
                {INSTALLS.map((install) => (
                  <TabsContent key={install.id} value={install.id}>
                    <div className="flex items-center justify-between gap-2 rounded-lg border border-white/10 bg-black/40 py-1 pl-4">
                      <code className="overflow-x-auto font-mono text-[13px] whitespace-nowrap text-foreground">
                        <span aria-hidden="true" className="text-zinc-500 select-none">
                          ${" "}
                        </span>
                        {install.command}
                      </code>
                      <CopyButton
                        text={install.command}
                        label={`Copy the ${install.label} install command`}
                      />
                    </div>
                  </TabsContent>
                ))}
              </Tabs>

              <div className="flex flex-wrap items-center gap-3">
                <Link href="/analyzer" className={cn(buttonVariants({ size: "lg" }), "px-4")}>
                  Open the analyzer
                </Link>
                <a
                  href="https://github.com/agnelnieves/tolkin"
                  rel="noopener"
                  className={cn(buttonVariants({ variant: "outline", size: "lg" }), "px-4")}
                >
                  GitHub
                </a>
              </div>
            </div>
          </div>

          <div data-hero-stagger>
            <Terminal startDelay={700} />
          </div>
        </div>
      </div>
      <Hairline />
    </section>
  );
}
