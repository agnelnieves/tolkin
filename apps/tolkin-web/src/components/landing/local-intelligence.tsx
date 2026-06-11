import { CodeBlock } from "./code-block";
import { SectionHeading } from "./section-heading";

const OPTIMIZE_OUTPUT = `$ tolkin optimize

  context files        22
  context tokens       44,174
  identified savings   774 to 2,201 tokens (advisory estimate)

No local model detected (checked 127.0.0.1 ports 8080, 11434, 1234).
The deterministic report above is complete without one.

Suggested model for this machine: mlx-community/Qwen3.5-4B-4bit (3.0 GB download)

Would you like to see how to install and set it up? [y/N]`;

const SETUP_SNIPPET = `brew install uv
uv tool install mlx-lm
mlx_lm.server --model mlx-community/Qwen3.5-4B-4bit --max-tokens 1200
tolkin optimize`;

const GATES = [
  "every digit in model output must exist in the deterministic input",
  "every quoted span must exist verbatim in the source file",
  "loopback only: 127.0.0.1 is enforced in code",
  'output is labeled "model advisory (local)", never a tier',
] as const;

const HARDWARE = [
  { ram: "8GB", model: "Qwen3.5-0.8B" },
  { ram: "16GB", model: "Qwen3.5-4B" },
  { ram: "32GB", model: "Qwen3.5-9B" },
  { ram: "48GB+", model: "Qwen3.5-35B-A3B MoE" },
] as const;

export function LocalIntelligence() {
  return (
    <section id="local" className="scroll-mt-20 border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Local intelligence"
          title="A local model, when you want one. Never required."
          lede="tolkin optimize runs deterministically on its own. If a local OpenAI-compatible server is already running (mlx-lm, Ollama, LM Studio, llama-server) and you consent, it adds plain-language narration and skill lint on top. Nothing leaves the machine."
        />

        <div className="mt-14 grid grid-cols-1 items-start gap-10 lg:grid-cols-2 lg:gap-14">
          <div className="flex flex-col gap-8">
            <p className="text-sm leading-relaxed text-muted-foreground">
              The model proposes, Rust verifies. Every model claim has to pass verification gates
              before it reaches your terminal; anything that fails is dropped and counted, not
              printed.
            </p>
            <ul className="flex flex-col gap-3">
              {GATES.map((gate) => (
                <li key={gate} className="flex gap-3 font-mono text-xs leading-relaxed">
                  <span aria-hidden="true" className="mt-px text-lime-300 select-none">
                    gate
                  </span>
                  <span className="text-muted-foreground">{gate}</span>
                </li>
              ))}
            </ul>
            <p className="text-sm leading-relaxed text-muted-foreground">
              When no model is installed, optimize does not dead-end. It detects the machine&apos;s
              RAM and chip, recommends the blessed model for that tier with the download size and a
              per-machine time estimate, and asks one question before showing the setup guide.
            </p>
          </div>

          <CodeBlock code={OPTIMIZE_OUTPUT} lang="bash" label="tolkin optimize" />
        </div>

        <div className="mt-12 grid grid-cols-1 items-start gap-10 lg:grid-cols-2 lg:gap-14">
          <CodeBlock
            code={SETUP_SNIPPET}
            lang="bash"
            label="setup, Apple silicon"
            copyText={SETUP_SNIPPET}
          />

          <div className="flex flex-col gap-4">
            <h3 className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
              Blessed models by machine
            </h3>
            <dl className="divide-y divide-white/10 border-y border-white/10 font-mono text-[13px]">
              {HARDWARE.map((row) => (
                <div key={row.ram} className="flex items-baseline justify-between gap-4 py-3">
                  <dt className="text-muted-foreground tabular-nums">{row.ram}</dt>
                  <dd className="text-foreground">{row.model}</dd>
                </div>
              ))}
            </dl>
            <p className="font-mono text-xs leading-relaxed text-muted-foreground">
              measured near 250 tok/s prefill on an M1 Max; estimates printed before any run
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
