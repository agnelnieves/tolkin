import { Analyzer } from "./analyzer";
import { CoreVersion } from "./core-version";
import { McpAnalyzer } from "./mcp-analyzer";

export default function Home() {
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
