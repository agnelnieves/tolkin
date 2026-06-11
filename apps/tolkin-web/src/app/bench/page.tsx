import type { Metadata } from "next";
import rawResults from "../../data/bench-results.json";
import type {
  BenchResults,
  ConfigurationCase,
  ExternalComparison,
  LossyCase,
  StructuralCase,
} from "../../types/bench";
import { BenchNav } from "../../components/bench/bench-nav";
import type { ClaimRow } from "../../components/bench/claim-bar";
import { ClaimBarSection } from "../../components/bench/claim-bar";
import {
  FidelityBadge,
  HonestyLabel,
  PendingNote,
  SectionDivider,
  Td,
  Th,
  TrackEyebrow,
  TrackTable,
} from "../../components/bench/track-section";

// Static import: the benchmark harness writes this file at build time.
// No network access, no browser storage. Works with both the placeholder
// shape (runs=0) and the real data shape.
const results = rawResults as BenchResults;

export const metadata: Metadata = {
  title: "Benchmarks",
  description:
    "Every number on this page was produced by a runnable harness with pinned fixtures. Claims measured against declared baselines, honest labels on every row.",
  alternates: {
    canonical: "/bench",
  },
  openGraph: {
    title: "Benchmarks",
    description:
      "Every number on this page was produced by a runnable harness with pinned fixtures. Claims measured against declared baselines, honest labels on every row.",
    url: "/bench",
  },
  twitter: {
    card: "summary_large_image",
    title: "Benchmarks",
    description:
      "Every number on this page was produced by a runnable harness with pinned fixtures. Claims measured against declared baselines, honest labels on every row.",
  },
};

// ---- formatting helpers ----

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function pct(n: number): string {
  return `${n.toFixed(1)}%`;
}

// ---- markdown renderer for methodology_md ----
// Only handles the subset the harness emits: h2, bold, list items, paragraphs.

type Block =
  | { kind: "h2"; text: string }
  | { kind: "p"; html: string }
  | { kind: "li"; html: string };

function parseInline(raw: string): string {
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

// ---- claim rows derived from the lossy comparisons data ----

function buildClaimRows(comparisons: ExternalComparison[]): ClaimRow[] {
  return comparisons
    .filter((c) => c.status === "measured" || c.status === "not-runnable-headless")
    .map((c) => {
      // Extract the short tool name from the full comparison name string
      const toolName = c.name.split(" ")[0] ?? c.name;

      // Claimed figures sourced directly from the methodology_md / notes.
      // cavemem: "about 75 percent fewer prose tokens" per upstream README.
      // repomix: "about 70 percent" per upstream README (no methodology disclosed).
      let claimed_pct: number | null = null;
      let claimed_label = "no public figure";

      if (toolName.startsWith("cavemem")) {
        claimed_pct = 75;
        claimed_label = "about 75% (upstream README, no methodology)";
      } else if (toolName.startsWith("repomix")) {
        claimed_pct = 70;
        claimed_label = "about 70% (upstream README, no methodology)";
      } else if (toolName.startsWith("caveman-shrink")) {
        claimed_pct = null;
        claimed_label = "no public figure";
      } else if (toolName.startsWith("wilpel/caveman-compression")) {
        claimed_pct = null;
        claimed_label = "no public figure";
      }

      const measured_pct = c.savings_pct ?? 0;

      let condition = c.notes ?? c.reason ?? "";
      // Truncate very long condition text for the bar display
      if (condition.length > 200) {
        condition = condition.slice(0, 197) + "...";
      }

      const statusMap: Record<string, ClaimRow["status"]> = {
        measured: "measured",
        "not-runnable-headless": "not-runnable",
        "not-comparable": "not-comparable",
      };

      return {
        tool: toolName,
        claimed_pct,
        claimed_label,
        measured_pct,
        condition,
        status: statusMap[c.status] ?? "measured",
        reason: c.reason,
      } satisfies ClaimRow;
    });
}

// ---- structural track table ----

function StructuralRows({ cases }: { cases: StructuralCase[] }) {
  return (
    <>
      {cases.map((c) => (
        <tr
          key={c.id}
          className="border-b border-white/[0.06] last:border-b-0 [@media(hover:hover)]:hover:bg-white/[0.02]"
        >
          <Td mono>{c.fixture.split("/").pop()}</Td>
          <Td dim>{c.technique}</Td>
          <Td dim>{c.tokenizer}</Td>
          <Td right mono dim>{fmt(c.before_tokens)}</Td>
          <Td right mono dim>{fmt(c.after_tokens)}</Td>
          <Td right mono accent>{fmt(c.savings_tokens)}</Td>
          <Td right mono accent>{pct(c.savings_pct)}</Td>
          <Td dim>
            {c.variance.min === c.variance.max
              ? "stable"
              : `${fmt(c.variance.min)}-${fmt(c.variance.max)}`}
          </Td>
        </tr>
      ))}
    </>
  );
}

// ---- configuration track table ----

function ConfigRows({ cases }: { cases: ConfigurationCase[] }) {
  return (
    <>
      {cases.map((c) => (
        <tr
          key={c.id}
          className="border-b border-white/[0.06] last:border-b-0 [@media(hover:hover)]:hover:bg-white/[0.02]"
        >
          <Td mono>{c.id.split("/").pop()}</Td>
          <Td dim>{c.basis}</Td>
          <Td dim>{c.tokenizer}</Td>
          <Td right mono dim>{c.tools}</Td>
          <Td right mono dim>{fmt(c.cold_tokens)}</Td>
          <Td right mono accent>{fmt(c.swap_savings_tokens)}</Td>
          <Td right mono accent>{fmt(c.slim_savings_tokens)}</Td>
          <Td right mono dim>{pct(c.pct_of_200k_window)}</Td>
        </tr>
      ))}
    </>
  );
}

// ---- lossy track table ----

function LossyRows({ cases }: { cases: LossyCase[] }) {
  return (
    <>
      {cases.map((c) => (
        <tr
          key={c.id}
          className="border-b border-white/[0.06] last:border-b-0 [@media(hover:hover)]:hover:bg-white/[0.02]"
        >
          <Td mono>{c.fixture.split("/").pop()}</Td>
          <Td dim>{c.technique.split(" ")[0]}</Td>
          <Td right mono dim>{c.target_ratio.toFixed(2)}</Td>
          <Td right mono dim>{fmt(c.before_tokens)}</Td>
          <Td right mono dim>{fmt(c.after_tokens)}</Td>
          <Td right mono dim>{c.achieved_ratio.toFixed(2)}</Td>
          <Td right mono accent>{pct(c.savings_pct)}</Td>
        </tr>
      ))}
    </>
  );
}

// ---- external comparisons table ----

function ExternalRows({ comparisons }: { comparisons: ExternalComparison[] }) {
  const measured = comparisons.filter((c) => c.status === "measured");
  const nonRunnable = comparisons.filter((c) => c.status !== "measured");

  return (
    <div className="space-y-4">
      {measured.length > 0 && (
        <TrackTable
          head={
            <>
              <Th>Tool</Th>
              <Th right>Before</Th>
              <Th right>After</Th>
              <Th right>Savings</Th>
              <Th>Notes</Th>
            </>
          }
          body={
            <>
              {measured.map((c) => (
                <tr
                  key={c.name}
                  className="border-b border-white/[0.06] last:border-b-0 [@media(hover:hover)]:hover:bg-white/[0.02]"
                >
                  <Td mono>{c.name.split(" ")[0]}</Td>
                  <Td right mono dim>
                    {c.before_tokens != null ? fmt(c.before_tokens) : "-"}
                  </Td>
                  <Td right mono dim>
                    {c.after_tokens != null ? fmt(c.after_tokens) : "-"}
                  </Td>
                  <Td right mono accent>
                    {c.savings_pct != null ? `${c.savings_pct.toFixed(2)}%` : "-"}
                  </Td>
                  <td className="px-3 py-2.5 text-xs text-zinc-500">{c.notes ?? "-"}</td>
                </tr>
              ))}
            </>
          }
        />
      )}

      {nonRunnable.length > 0 && (
        <ul className="space-y-2 text-xs">
          {nonRunnable.map((c) => (
            <li key={c.name} className="rounded-lg border border-white/[0.06] bg-white/[0.01] p-3">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-zinc-300">{c.name.split(" ")[0]}</span>
                <HonestyLabel status={c.status} reason={c.reason} />
              </div>
              <p className="mt-1.5 leading-5 text-zinc-500">{c.reason}</p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---- manifest provenance list ----

function ManifestProvenance({ cases }: { cases: ConfigurationCase[] }) {
  return (
    <div className="space-y-2">
      <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
        Manifest provenance
      </p>
      <ul className="space-y-1.5 text-[11px] leading-5 text-zinc-500">
        {cases.map((c) => (
          <li key={c.id}>
            <span className="font-mono text-zinc-400">{c.id}</span>: {c.manifest.source};{" "}
            {c.manifest.server_version}; captured {c.manifest.captured}; {c.manifest.license}.
          </li>
        ))}
      </ul>
    </div>
  );
}

// ---- methodology renderer ----

function MethodologySection({ md }: { md: string }) {
  const blocks = renderMethodology(md);
  return (
    <div className="space-y-3 text-sm leading-6 text-zinc-400">
      {blocks.map((block, i) => {
        if (block.kind === "h2") {
          return (
            <h3
              key={`h-${i}`}
              className="mt-6 text-sm font-medium text-zinc-300 first:mt-0"
            >
              {block.text}
            </h3>
          );
        }
        if (block.kind === "li") {
          return (
            <li
              key={`li-${i}`}
              className="ml-4 list-disc text-zinc-400"
              // Safe: parseInline only inserts <strong> tags from the harness-emitted markdown
              dangerouslySetInnerHTML={{ __html: block.html }}
            />
          );
        }
        return (
          <p
            key={`p-${i}`}
            className="text-zinc-400"
            // Safe: parseInline only inserts <strong> tags from the harness-emitted markdown
            dangerouslySetInnerHTML={{ __html: block.html }}
          />
        );
      })}
    </div>
  );
}

// ---- page ----

export default function BenchPage() {
  const { generated_at, tolkin_version, prices_observed, runs, methodology_md, tracks } = results;

  const isPending = runs === 0;
  const generatedDate = isPending ? null : new Date(generated_at).toISOString().slice(0, 10);

  // Claim rows for the headline visual: lossy comparisons have the most notable
  // claimed-vs-measured deltas (cavemem 75% claimed / 11.72% measured,
  // repomix 70% claimed / 45.47% measured).
  const lossyClaimRows = buildClaimRows(tracks.lossy.comparisons);
  const hasLossyClaims = lossyClaimRows.some((r) => r.claimed_pct !== null);

  return (
    <div className="min-h-screen bg-[#0c0c0b] text-white">
      <BenchNav />

      <main>
        {/* ---- Hero header ---- */}
        <section className="border-b border-white/[0.06]">
          <div className="mx-auto max-w-[1200px] px-6 py-16 sm:py-24">
            <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
              Benchmarks
            </p>
            <h1 className="mt-4 font-display text-4xl font-semibold tracking-tight text-balance sm:text-5xl md:text-6xl">
              Claims, measured.
            </h1>
            <p className="mt-5 max-w-2xl text-base leading-relaxed text-zinc-400 sm:text-lg">
              Every number on this page was produced by a runnable harness with pinned fixtures.
              Labels say what was measured and what was not.
            </p>
            <p className="mt-4 font-mono text-xs text-zinc-600">
              {isPending ? (
                "results pending"
              ) : (
                <>
                  generated {generatedDate} &middot; tolkin {tolkin_version} &middot; prices{" "}
                  {prices_observed} &middot; {runs} {runs === 1 ? "run" : "runs"}
                </>
              )}
            </p>
          </div>
        </section>

        {/* ---- Claimed vs measured visual ---- */}
        {hasLossyClaims && (
          <section className="border-b border-white/[0.06]" aria-labelledby="claims-heading">
            <div className="mx-auto max-w-[1200px] px-6 py-14 sm:py-20">
              <TrackEyebrow label="Claimed vs. Measured" />
              <h2
                id="claims-heading"
                className="mt-4 font-display text-2xl font-semibold tracking-tight sm:text-3xl"
              >
                The gap is the point.
              </h2>
              <p className="mt-3 max-w-2xl text-sm leading-relaxed text-zinc-400">
                Published claims from upstream READMEs, run against the same declared fixtures, same
                tokenizer, same one-command harness. The difference is denominator choice, not
                fabrication.
              </p>

              <div className="mt-10 max-w-3xl">
                <ClaimBarSection rows={lossyClaimRows.filter((r) => r.claimed_pct !== null)} />
              </div>

              <div className="mt-8 space-y-1">
                <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
                  Label key
                </p>
                <ul className="flex flex-wrap gap-3 text-xs text-zinc-500">
                  <li className="flex items-center gap-1.5">
                    <span className="inline-block rounded border border-lime-400/40 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-lime-300">
                      measured
                    </span>
                    Run on the same fixtures, same tokenizer.
                  </li>
                  <li className="flex items-center gap-1.5">
                    <span className="inline-block rounded border border-white/10 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-zinc-500">
                      not-runnable
                    </span>
                    Cannot be run headlessly; reason in the comparisons table below.
                  </li>
                </ul>
              </div>
            </div>
          </section>
        )}

        {/* ---- Track sections ---- */}
        <div className="mx-auto max-w-[1200px] divide-y divide-white/[0.06] px-6">

          {/* ---- Structural track ---- */}
          <section className="py-14 sm:py-20" aria-labelledby="structural-heading">
            <TrackEyebrow label="Track 1" />
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <h2
                id="structural-heading"
                className="font-display text-2xl font-semibold tracking-tight sm:text-3xl"
              >
                Structural
              </h2>
              <FidelityBadge fidelity={tracks.structural.fidelity} />
            </div>
            <p className="mt-3 max-w-2xl text-sm leading-relaxed text-zinc-400">
              Whitespace normalization, comment stripping, and format conversions applied to
              real-world fixtures. All savings are lossless: semantic content is preserved exactly.
              Deterministic transforms must show zero variance across runs; the harness treats any
              deviation as a failure.
            </p>

            <div className="mt-8">
              {tracks.structural.cases.length === 0 ? (
                <PendingNote />
              ) : (
                <TrackTable
                  head={
                    <>
                      <Th>Case</Th>
                      <Th>Technique</Th>
                      <Th>Tokenizer</Th>
                      <Th right>Before</Th>
                      <Th right>After</Th>
                      <Th right>Saved</Th>
                      <Th right>Pct</Th>
                      <Th>Variance</Th>
                    </>
                  }
                  body={<StructuralRows cases={tracks.structural.cases} />}
                />
              )}
            </div>
          </section>

          <SectionDivider />

          {/* ---- Configuration track ---- */}
          <section className="py-14 sm:py-20" aria-labelledby="config-heading">
            <TrackEyebrow label="Track 2" />
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <h2
                id="config-heading"
                className="font-display text-2xl font-semibold tracking-tight sm:text-3xl"
              >
                Configuration
              </h2>
              <FidelityBadge fidelity={tracks.configuration.fidelity} />
            </div>
            <p className="mt-3 max-w-2xl text-sm leading-relaxed text-zinc-400">
              Every MCP server in a client configuration contributes its tool definitions to every
              request. This track counts that weight using real, vendored tools/list manifests
              captured live from public servers and tokenized with o200k_base. Cold is the tokenized
              weight of the tool definitions, no price multipliers. Swap savings replace a server
              with a CLI equivalent; slim savings are the measured difference between two tokenized
              manifests.
            </p>

            <div className="mt-8 space-y-6">
              {tracks.configuration.cases.length === 0 ? (
                <PendingNote />
              ) : (
                <TrackTable
                  head={
                    <>
                      <Th>Case</Th>
                      <Th>Basis</Th>
                      <Th>Tokenizer</Th>
                      <Th right>Tools</Th>
                      <Th right>Cold tokens</Th>
                      <Th right>Swap savings</Th>
                      <Th right>Slim savings</Th>
                      <Th right>% of 200K</Th>
                    </>
                  }
                  body={<ConfigRows cases={tracks.configuration.cases} />}
                />
              )}

              <ManifestProvenance cases={tracks.configuration.cases} />

              {tracks.configuration.comparisons.length > 0 && (
                <div className="space-y-3">
                  <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
                    External comparisons
                  </p>
                  <ExternalRows comparisons={tracks.configuration.comparisons} />
                </div>
              )}
            </div>
          </section>

          <SectionDivider />

          {/* ---- Lossy track ---- */}
          <section className="py-14 sm:py-20" aria-labelledby="lossy-heading">
            <TrackEyebrow label="Track 3" />
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <h2
                id="lossy-heading"
                className="font-display text-2xl font-semibold tracking-tight sm:text-3xl"
              >
                Lossy
              </h2>
              <FidelityBadge fidelity={tracks.lossy.fidelity} />
            </div>
            <p className="mt-3 max-w-2xl text-sm leading-relaxed text-zinc-400">
              LLMLingua-2 compression at declared target ratios, measured on real prose fixtures.
              Every case publishes both the target ratio and the achieved ratio. Quality scoring is
              off by default and requires your own API key.
            </p>

            <div className="mt-6 rounded-lg border border-amber-900/50 bg-amber-950/20 p-4">
              <p className="text-xs leading-5 text-amber-300">{tracks.lossy.rct_caveat}</p>
            </div>

            <div className="mt-8 space-y-6">
              {tracks.lossy.cases.length === 0 ? (
                <PendingNote />
              ) : (
                <TrackTable
                  head={
                    <>
                      <Th>Case</Th>
                      <Th>Technique</Th>
                      <Th right>Target ratio</Th>
                      <Th right>Before</Th>
                      <Th right>After</Th>
                      <Th right>Achieved ratio</Th>
                      <Th right>Savings</Th>
                    </>
                  }
                  body={<LossyRows cases={tracks.lossy.cases} />}
                />
              )}

              <div className="rounded-lg border border-white/[0.06] bg-white/[0.01] p-4">
                <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
                  Quality scoring
                </p>
                <p className="mt-2 text-xs leading-5 text-zinc-500">
                  scored={tracks.lossy.quality_scoring.scored ? "true" : "false"}. Method:{" "}
                  {tracks.lossy.quality_scoring.method}
                </p>
              </div>

              {tracks.lossy.comparisons.length > 0 && (
                <div className="space-y-3">
                  <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-zinc-500">
                    External comparisons
                  </p>
                  <ExternalRows comparisons={tracks.lossy.comparisons} />
                </div>
              )}
            </div>
          </section>

          <SectionDivider />

          {/* ---- Methodology ---- */}
          <section className="py-14 sm:py-20" aria-labelledby="methodology-heading">
            <TrackEyebrow label="Methodology" />
            <h2
              id="methodology-heading"
              className="mt-4 font-display text-2xl font-semibold tracking-tight sm:text-3xl"
            >
              How the numbers are made.
            </h2>
            <p className="mt-3 max-w-2xl text-sm leading-relaxed text-zinc-400">
              The full methodology text below is generated by the same harness that produces the
              numbers. It is the single source of truth shared by this page and RESULTS.md.
            </p>
            <div className="mt-8 max-w-3xl">
              <MethodologySection md={methodology_md} />
            </div>
          </section>
        </div>

        {/* ---- Footer provenance block ---- */}
        <footer className="border-t border-white/[0.06]">
          <div className="mx-auto max-w-[1200px] space-y-2 px-6 py-10 font-mono text-[11px] leading-6 text-zinc-600">
            <p>All savings are input-token bounded. Output behavior is not measured or claimed.</p>
            {!isPending && (
              <>
                <p>
                  Environment: {results.environment.runner} on {results.environment.os}.
                </p>
                <p>
                  Tokenizers: o200k_base exact (OpenAI); cl100k_base proxy +/-10% estimate
                  (Anthropic); Gemma SPM exact (Gemini).
                </p>
                <p>
                  Generated {generatedDate} &middot; tolkin {tolkin_version} &middot; prices{" "}
                  {prices_observed} &middot; {runs} {runs === 1 ? "run" : "runs"}.
                </p>
              </>
            )}
          </div>
        </footer>
      </main>
    </div>
  );
}
