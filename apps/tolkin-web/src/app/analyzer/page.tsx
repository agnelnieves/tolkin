import type { Metadata } from "next";
import { Analyzer } from "../analyzer";
import { CoreVersion } from "../core-version";
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
// provides the route and metadata.
export default function AnalyzerPage() {
  return (
    <main className="min-h-screen bg-black text-white">
      <div className="mx-auto flex max-w-4xl flex-col gap-10 px-6 py-12 sm:py-16">
        <header className="space-y-3">
          <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">Tolkin</h1>
          <p className="text-sm text-zinc-400 sm:text-base">
            Privacy-first AI token analyzer. Nothing leaves your browser.
          </p>
          <p className="text-xs font-mono text-zinc-600">
            v0.1.0 (scaffolding). <CoreVersion />
          </p>
        </header>

        <Analyzer />

        <hr className="border-zinc-900" />

        <McpAnalyzer />
      </div>
    </main>
  );
}
