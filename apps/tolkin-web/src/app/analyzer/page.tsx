import type { Metadata } from "next";
import { AnalyzerTopBar } from "@/components/analyzer/top-bar";
import { Analyzer } from "../analyzer";
import { McpAnalyzer } from "../mcp-analyzer";

export const metadata: Metadata = {
  title: "Analyzer",
  description:
    "Paste text or drop files to count tokens across providers, redact secrets, estimate cost, and audit agent context. Runs entirely in your browser.",
  alternates: {
    canonical: "/analyzer",
  },
  openGraph: {
    title: "Analyzer",
    description:
      "Paste text or drop files to count tokens across providers, redact secrets, estimate cost, and audit agent context. Runs entirely in your browser.",
    url: "/analyzer",
  },
  twitter: {
    card: "summary_large_image",
    title: "Analyzer",
    description:
      "Paste text or drop files to count tokens across providers, redact secrets, estimate cost, and audit agent context. Runs entirely in your browser.",
  },
};

// The in-browser analyzer, previously the root page. Behavior is unchanged:
// the client components below own all state and workers; this page only
// provides the route, metadata, and the landing-aligned shell.
export default function AnalyzerPage() {
  return (
    <div className="dark landing min-h-screen bg-background text-foreground antialiased selection:bg-lime-300 selection:text-black">
      <a
        href="#analyzer-main"
        className="sr-only z-[60] rounded-md bg-lime-300 px-4 py-2 font-mono text-sm text-black focus:not-sr-only focus:fixed focus:top-3 focus:left-3"
      >
        Skip to content
      </a>

      <AnalyzerTopBar />

      <main
        id="analyzer-main"
        className="mx-auto w-full max-w-[1320px] px-4 pt-8 pb-16 sm:px-6 sm:pt-12"
      >
        <header className="mb-8 space-y-3 sm:mb-12">
          <h1 className="font-display text-3xl font-semibold tracking-tight text-balance sm:text-4xl md:text-5xl">
            Analyzer
          </h1>
          <p className="max-w-2xl text-base leading-relaxed text-muted-foreground sm:text-lg">
            Paste anything. Count everything. Nothing leaves your browser.
          </p>
        </header>

        <Analyzer />

        <div className="mt-12 border-t border-white/10 pt-12 sm:mt-16 sm:pt-16">
          <McpAnalyzer />
        </div>
      </main>
    </div>
  );
}
