import { CopyButton } from "./copy-button";
import { SectionHeading } from "./section-heading";

const COMMANDS = [
  { label: "Homebrew", command: "brew install agnelnieves/tolkin/tolkin" },
  { label: "npm", command: "npm install -g tolkin-cli" },
  { label: "bunx", command: "bunx tolkin" },
] as const;

const CHANNELS = [
  {
    name: "GitHub Action",
    body: "Delta gates and sticky PR comments: every pull request that touches agent context gets a before and after table.",
  },
  {
    name: "Agent skills",
    body: "Four skills (audit, slim, optimize, cache) for Claude Code, Cursor, Windsurf, Codex, and any client that reads SKILL.md.",
  },
  {
    name: "Claude Code plugin",
    body: "One marketplace install namespaces all four skills under /tolkin.",
  },
  {
    name: "tolkin update",
    body: "Knows your install channel: Homebrew users see brew upgrade, npm users see npm update.",
  },
] as const;

export function Distribution() {
  return (
    <section className="border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Distribution"
          title="Wherever your agents live."
        />

        <div className="mt-12 flex flex-col gap-3">
          {COMMANDS.map((entry) => (
            <div
              key={entry.label}
              className="flex items-center justify-between gap-3 rounded-lg border border-white/10 bg-black/40 py-1 pl-4"
            >
              <code className="overflow-x-auto font-mono text-[13px] whitespace-nowrap text-foreground">
                <span aria-hidden="true" className="text-zinc-500 select-none">
                  ${" "}
                </span>
                {entry.command}
              </code>
              <div className="flex items-center gap-1">
                <span className="hidden font-mono text-[11px] uppercase tracking-[0.16em] text-muted-foreground sm:inline">
                  {entry.label}
                </span>
                <CopyButton text={entry.command} label={`Copy the ${entry.label} install command`} />
              </div>
            </div>
          ))}
        </div>

        <dl className="mt-12 grid grid-cols-1 gap-x-10 gap-y-8 sm:grid-cols-2">
          {CHANNELS.map((channel) => (
            <div key={channel.name} className="flex flex-col gap-2 border-t border-white/10 pt-5">
              <dt className="font-mono text-sm text-foreground">{channel.name}</dt>
              <dd className="text-sm leading-relaxed text-muted-foreground">{channel.body}</dd>
            </div>
          ))}
        </dl>
      </div>
    </section>
  );
}
