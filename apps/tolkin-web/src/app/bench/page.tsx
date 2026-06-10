import type { Metadata } from "next";
import rawResults from "../../data/bench-results.json";
import type {
  BenchResults,
  ConfigurationCase,
  ExternalComparison,
  LossyCase,
  StructuralCase,
} from "../../types/bench";

// Static import: the benchmark harness writes this file and it is bundled at
// build time. No network access and no browser storage of any kind. The page
// renders correctly for both the placeholder shape (empty tracks, runs = 0)
// and the real data shape.
const results = rawResults as BenchResults;

export const metadata: Metadata = {
  title: "Benchmark",
  description:
    "Measured, reproducible input-token savings for Tolkin's lossless and lossy techniques. Configuration track uses catalog estimates. Methodology and raw numbers included.",
  alternates: {
    canonical: "/bench",
  },
  openGraph: {
    title: "Benchmark",
    description:
      "Measured, reproducible input-token savings for Tolkin's lossless and lossy techniques. Configuration track uses catalog estimates. Methodology and raw numbers included.",
    url: "/bench",
  },
  twitter: {
    card: "summary_large_image",
    title: "Benchmark",
    description:
      "Measured, reproducible input-token savings for Tolkin's lossless and lossy techniques. Configuration track uses catalog estimates. Methodology and raw numbers included.",
  },
};

// ---- formatting helpers ----

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function pct(n: number): string {
  return `${n.toFixed(1)}%`;
}

// ---- small markdown renderer for methodology_md ----
// Handles only the subset the harness emits: h2 (## ...), bold (**...**),
// unordered list items (- ...), and plain paragraphs. No external packages.

type Block =
  | { kind: "h2"; text: string }
  | { kind: "p"; html: string }
  | { kind: "li"; html: string };

function parseInline(raw: string): string {
  // **bold** only; no em-dashes, no other inline markup from the harness.
  return raw.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
}

function renderMethodology(md: string): Block[] {
  const lines = md.split("\n");
  const blocks: Block[] = [];
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (line.startsWith("## ")) {
      blocks.push({ kind: "h2", text: line.slice(3).trim() });
    } else if (line.startsWith("- ")) {
      blocks.push({ kind: "li", html: parseInline(line.slice(2).trim()) });
    } else if (line.trim() !== "") {
      blocks.push({ kind: "p", html: parseInline(line.trim()) });
    }
  }
  return blocks;
}

// ---- fidelity badge ----

function FidelityBadge({ fidelity }: { fidelity: string }) {
  const styles: Record<string, string> = {
    lossless: "bg-emerald-950 text-emerald-300",
    "lossless-configuration": "bg-sky-950 text-sky-300",
    lossy: "bg-amber-950 text-amber-300",
  };
  const labels: Record<string, string> = {
    lossless: "lossless",
    "lossless-configuration": "lossless-config",
    lossy: "lossy",
  };
  return (
    <span
      className={`rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider ${styles[fidelity] ?? "bg-zinc-800 text-zinc-400"}`}
    >
      {labels[fidelity] ?? fidelity}
    </span>
  );
}

// ---- empty track note ----

function PendingNote() {
  return (
    <p className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4 text-xs text-zinc-500">
      Results pending. Run the harness at apps/tolkin-cli/benchmarks to populate this track.
    </p>
  );
}

// ---- external comparisons ----

function ExternalComparisons({ comparisons }: { comparisons: ExternalComparison[] }) {
  if (comparisons.length === 0) return null;

  const measured = comparisons.filter((c) => c.status === "measured");
  const nonRunnable = comparisons.filter((c) => c.status !== "measured");

  return (
    <div className="space-y-3">
      <h4 className="text-[11px] font-medium uppercase tracking-wider text-zinc-500">
        External comparisons
      </h4>

      {measured.length > 0 && (
        <div className="overflow-x-auto rounded-lg border border-zinc-800">
          <table className="w-full border-collapse text-left text-xs">
            <thead>
              <tr className="border-b border-zinc-800 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500">
                <th className="px-3 py-2 font-medium">Name</th>
                <th className="px-3 py-2 text-right font-medium">Before</th>
                <th className="px-3 py-2 text-right font-medium">After</th>
                <th className="px-3 py-2 text-right font-medium">Savings</th>
                <th className="px-3 py-2 font-medium">Notes</th>
              </tr>
            </thead>
            <tbody>
              {measured.map((c) => (
                <tr key={c.name} className="border-b border-zinc-900 last:border-b-0">
                  <td className="px-3 py-2 font-mono text-zinc-200">{c.name}</td>
                  <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                    {c.before_tokens != null ? fmt(c.before_tokens) : "-"}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                    {c.after_tokens != null ? fmt(c.after_tokens) : "-"}
                  </td>
                  <td className="px-3 py-2 text-right tabular-nums text-emerald-300">
                    {c.savings_pct != null ? pct(c.savings_pct) : "-"}
                  </td>
                  <td className="px-3 py-2 text-zinc-500">{c.notes ?? "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {nonRunnable.length > 0 && (
        <ul className="space-y-1 text-xs">
          {nonRunnable.map((c) => (
            <li key={c.name} className="flex flex-wrap gap-2 text-zinc-500">
              <span className="font-mono text-zinc-300">{c.name}</span>
              <span className="rounded bg-zinc-800 px-1 py-0.5 text-[10px] uppercase tracking-wider text-zinc-500">
                {c.status}
              </span>
              <span>{c.reason}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---- structural track ----

function StructuralSection({ cases }: { cases: StructuralCase[] }) {
  if (cases.length === 0) return <PendingNote />;

  return (
    <div className="overflow-x-auto rounded-lg border border-zinc-800">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-zinc-800 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500">
            <th className="px-3 py-2 font-medium">Case</th>
            <th className="px-3 py-2 font-medium">Technique</th>
            <th className="px-3 py-2 font-medium">Tokenizer</th>
            <th className="px-3 py-2 text-right font-medium">Before</th>
            <th className="px-3 py-2 text-right font-medium">After</th>
            <th className="px-3 py-2 text-right font-medium">Saved</th>
            <th className="px-3 py-2 text-right font-medium">Pct</th>
            <th className="px-3 py-2 font-medium">Variance</th>
          </tr>
        </thead>
        <tbody>
          {cases.map((c) => (
            <tr key={c.id} className="border-b border-zinc-900 last:border-b-0">
              <td className="px-3 py-2 font-mono text-zinc-200">{c.fixture}</td>
              <td className="px-3 py-2 text-zinc-300">{c.technique}</td>
              <td className="px-3 py-2 text-zinc-400">{c.tokenizer}</td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {fmt(c.before_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {fmt(c.after_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-emerald-300">
                {fmt(c.savings_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-emerald-200">
                {pct(c.savings_pct)}
              </td>
              <td className="px-3 py-2 text-zinc-500">
                {c.variance.min === c.variance.max
                  ? "stable"
                  : `${fmt(c.variance.min)}-${fmt(c.variance.max)}`}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---- configuration track ----

function ConfigurationSection({ cases }: { cases: ConfigurationCase[] }) {
  if (cases.length === 0) return <PendingNote />;

  return (
    <div className="overflow-x-auto rounded-lg border border-zinc-800">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-zinc-800 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500">
            <th className="px-3 py-2 font-medium">Case</th>
            <th className="px-3 py-2 font-medium">Shape</th>
            <th className="px-3 py-2 text-right font-medium">Servers</th>
            <th className="px-3 py-2 text-right font-medium">Cold tokens</th>
            <th className="px-3 py-2 text-right font-medium">Swap savings</th>
            <th className="px-3 py-2 text-right font-medium">Slim savings</th>
            <th className="px-3 py-2 text-right font-medium">% of 200K</th>
          </tr>
        </thead>
        <tbody>
          {cases.map((c) => (
            <tr key={c.id} className="border-b border-zinc-900 last:border-b-0">
              <td className="px-3 py-2 font-mono text-zinc-200">{c.fixture}</td>
              <td className="px-3 py-2 text-zinc-300">{c.client_shape}</td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">{c.servers}</td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {fmt(c.cold_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-emerald-300">
                {fmt(c.swap_savings_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-emerald-300">
                {fmt(c.slim_savings_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {pct(c.pct_of_200k_window)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---- lossy track ----

function LossySection({ cases }: { cases: LossyCase[] }) {
  if (cases.length === 0) return <PendingNote />;

  return (
    <div className="overflow-x-auto rounded-lg border border-zinc-800">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-zinc-800 bg-zinc-900/40 text-[10px] uppercase tracking-wider text-zinc-500">
            <th className="px-3 py-2 font-medium">Case</th>
            <th className="px-3 py-2 font-medium">Technique</th>
            <th className="px-3 py-2 text-right font-medium">Target ratio</th>
            <th className="px-3 py-2 text-right font-medium">Before</th>
            <th className="px-3 py-2 text-right font-medium">After</th>
            <th className="px-3 py-2 text-right font-medium">Achieved ratio</th>
            <th className="px-3 py-2 text-right font-medium">Savings</th>
          </tr>
        </thead>
        <tbody>
          {cases.map((c) => (
            <tr key={c.id} className="border-b border-zinc-900 last:border-b-0">
              <td className="px-3 py-2 font-mono text-zinc-200">{c.fixture}</td>
              <td className="px-3 py-2 text-zinc-300">{c.technique}</td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {c.target_ratio.toFixed(2)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {fmt(c.before_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {fmt(c.after_tokens)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-zinc-400">
                {c.achieved_ratio.toFixed(2)}
              </td>
              <td className="px-3 py-2 text-right tabular-nums text-emerald-300">
                {pct(c.savings_pct)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---- page ----

export default function BenchPage() {
  const { generated_at, tolkin_version, prices_observed, runs, methodology_md, tracks } = results;

  const isPending = runs === 0;
  const generatedDate = isPending ? null : new Date(generated_at).toISOString().slice(0, 10);
  const methodologyBlocks = renderMethodology(methodology_md);

  return (
    <main className="min-h-screen bg-black text-white">
      <div className="mx-auto flex max-w-4xl flex-col gap-10 px-6 py-12 sm:py-16">
        {/* Header */}
        <header className="space-y-3">
          <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">Benchmark</h1>
          <p className="text-sm text-zinc-400 sm:text-base">
            Measured, reproducible, input-token bounded. Configuration track uses catalog estimates.
          </p>
          <p className="text-xs font-mono text-zinc-600">
            {isPending ? (
              "results pending"
            ) : (
              <>
                generated {generatedDate} &middot; tolkin {tolkin_version} &middot; prices{" "}
                {prices_observed} &middot; {runs} {runs === 1 ? "run" : "runs"}
              </>
            )}
          </p>
        </header>

        <hr className="border-zinc-900" />

        {/* Methodology */}
        <section className="space-y-4">
          <h2 className="text-sm font-medium text-zinc-300">Methodology</h2>
          <div className="space-y-3 text-sm leading-6 text-zinc-400">
            {methodologyBlocks.map((block, i) => {
              if (block.kind === "h2") {
                return (
                  <h3 key={`h-${i}`} className="mt-6 text-sm font-medium text-zinc-300 first:mt-0">
                    {block.text}
                  </h3>
                );
              }
              if (block.kind === "li") {
                return (
                  <li
                    key={`li-${i}`}
                    className="ml-4 list-disc text-zinc-400"
                    dangerouslySetInnerHTML={{ __html: block.html }}
                  />
                );
              }
              return (
                <p
                  key={`p-${i}`}
                  className="text-zinc-400"
                  dangerouslySetInnerHTML={{ __html: block.html }}
                />
              );
            })}
          </div>
        </section>

        <hr className="border-zinc-900" />

        {/* Structural track */}
        <section className="space-y-4">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="text-sm font-medium text-zinc-300">Structural</h2>
            <FidelityBadge fidelity={tracks.structural.fidelity} />
          </div>
          <p className="text-xs text-zinc-500">
            Whitespace normalization, comment stripping, and format conversions. All savings are
            lossless: the semantic content is preserved exactly.
          </p>
          <StructuralSection cases={tracks.structural.cases} />
        </section>

        <hr className="border-zinc-900" />

        {/* Configuration track */}
        <section className="space-y-4">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="text-sm font-medium text-zinc-300">Configuration</h2>
            <FidelityBadge fidelity={tracks.configuration.fidelity} />
          </div>
          <p className="text-xs text-zinc-500">
            MCP server and agent config analysis: cold-session token cost, CLI swap savings, and
            slim-profile savings per fixture. Numbers are representative catalog estimates, not
            tokenized manifests. Refreshable when the catalog is updated.
          </p>
          <ConfigurationSection cases={tracks.configuration.cases} />
          <ExternalComparisons comparisons={tracks.configuration.comparisons} />
        </section>

        <hr className="border-zinc-900" />

        {/* Lossy track */}
        <section className="space-y-4">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="text-sm font-medium text-zinc-300">Lossy</h2>
            <FidelityBadge fidelity={tracks.lossy.fidelity} />
          </div>

          {/* RCT caveat banner */}
          <div className="rounded-lg border border-amber-900/60 bg-amber-950/20 p-4">
            <p className="text-xs leading-5 text-amber-300">{tracks.lossy.rct_caveat}</p>
          </div>

          <LossySection cases={tracks.lossy.cases} />

          {/* Quality scoring disclosure */}
          <p className="text-xs text-zinc-500">
            Quality scoring: scored={tracks.lossy.quality_scoring.scored ? "true" : "false"}.
            Method: {tracks.lossy.quality_scoring.method}.
          </p>

          <ExternalComparisons comparisons={tracks.lossy.comparisons} />
        </section>

        {/* Global footer */}
        <footer className="border-t border-zinc-900 pt-4 text-xs leading-5 text-zinc-600">
          <p>All savings are input-token bounded; output may vary.</p>
          {!isPending && (
            <p className="mt-1">
              Environment: {results.environment.runner} on {results.environment.os}.
            </p>
          )}
        </footer>
      </div>
    </main>
  );
}
