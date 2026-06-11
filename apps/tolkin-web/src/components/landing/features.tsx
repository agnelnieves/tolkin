import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { SectionHeading } from "./section-heading";

const FEATURES = [
  {
    name: "scan",
    body: "Finds every agent config on the machine: a dozen clients, instruction files, hooks, workflow LLM detectors.",
  },
  {
    name: "project",
    body: "Gitignore-aware repo context audit by load profile, with --fail-on for CI.",
  },
  {
    name: "audit",
    body: "13 rules, 6 production-proven and 7 experimental. Lighthouse-style findings with severity and citations.",
  },
  {
    name: "mcp",
    body: "Cold, warm, and Tool-Search scenarios, with slim and swap recommendations per server.",
  },
  {
    name: "cache",
    body: "Measured hit rate, write churn, and a 5m vs 1h TTL counterfactual replayed on your own logs.",
  },
  {
    name: "stats",
    body: "A local, consented ledger of savings over time. Three honesty tiers, never blended.",
  },
  {
    name: "report",
    body: "A self-contained HTML savings report you can hand to anyone, tier labels included.",
  },
  {
    name: "redact",
    body: "Always-on secret redaction before anything else sees a byte.",
  },
] as const;

export function Features() {
  return (
    <section id="features" className="scroll-mt-20 border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Features"
          title="Everything is measured."
          lede="Eight commands, one posture: deterministic output, honest labels, and nothing invented. The same Rust core runs in the CLI, the web analyzer, and the GitHub Action."
        />

        <div className="mt-14 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {FEATURES.map((feature) => (
            <Card key={feature.name} className="bg-white/[0.02] ring-white/10">
              <CardHeader>
                <CardTitle className="font-mono text-sm text-lime-300">{feature.name}</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="text-sm leading-relaxed text-muted-foreground">{feature.body}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    </section>
  );
}
