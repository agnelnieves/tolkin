import Link from "next/link";
import { PrivacyTag } from "./panel";

// Slim top bar for the analyzer route. Wordmark, the "Analyzer" tag, the
// privacy promise with a lime dot, and links out to the bench and GitHub.
// Kept under one row on every viewport; the privacy line collapses on small
// screens but the dot stays so the posture reads at a glance.
export function AnalyzerTopBar() {
  return (
    <header className="sticky top-0 z-40 border-b border-white/10 bg-background/85 backdrop-blur-md">
      <div className="mx-auto flex h-14 max-w-[1320px] items-center gap-3 px-4 sm:gap-6 sm:px-6">
        <div className="flex min-w-0 items-center gap-3">
          <Link
            href="/"
            className="font-display text-lg font-semibold tracking-tight text-foreground transition-opacity duration-150 ease-out hover:opacity-80"
          >
            tolkin
          </Link>
          <span
            aria-hidden="true"
            className="hidden h-4 w-px bg-white/10 sm:block"
          />
          <span className="hidden font-mono text-[11px] uppercase tracking-[0.22em] text-muted-foreground sm:inline">
            Analyzer
          </span>
        </div>

        <div className="ml-auto flex items-center gap-3 sm:gap-5">
          <PrivacyTag className="hidden md:inline-flex" />
          <span
            aria-hidden="true"
            className="hidden h-4 w-px bg-white/10 md:block"
          />
          <Link
            href="/bench"
            className="rounded-md px-2 py-1 font-mono text-xs text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
          >
            Bench
          </Link>
          <a
            href="https://github.com/agnelnieves/tolkin"
            rel="noopener"
            className="rounded-md px-2 py-1 font-mono text-xs text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
          >
            GitHub
          </a>
        </div>
      </div>
      <div className="mx-auto flex max-w-[1320px] items-center justify-between gap-4 px-4 pb-2 sm:hidden">
        <PrivacyTag />
      </div>
    </header>
  );
}
