import { SectionHeading } from "./section-heading";

const EGRESS_CLASSES = [
  {
    name: "byok verify",
    body: "Opt-in token verification: one endpoint, your key, your flag. Never runs on its own.",
  },
  {
    name: "update check",
    body: "One GET to the npm registry, either explicit (tolkin update) or consented at onboarding.",
  },
  {
    name: "mcp probe",
    body: "Speaks only to servers your own config already names. Per-server confirmation, refused in CI.",
  },
] as const;

const KILL_SWITCHES = [
  "TOLKIN_NO_SIDECAR",
  "TOLKIN_NO_UPDATE_CHECK",
  "TOLKIN_NO_LEDGER",
  "CI",
] as const;

export function Privacy() {
  return (
    <section id="privacy" className="scroll-mt-20 border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <SectionHeading
          eyebrow="Privacy"
          title="Zero egress is the default, not a setting."
          lede="Three classes of network traffic exist, each user-controlled and documented down to the implementing source file. Everything else: no network calls, ever."
        />

        <div className="mt-14 grid grid-cols-1 gap-px overflow-hidden rounded-xl border border-white/10 bg-white/10 md:grid-cols-3">
          {EGRESS_CLASSES.map((cls, i) => (
            <div key={cls.name} className="flex flex-col gap-3 bg-background p-8">
              <span className="font-mono text-xs text-muted-foreground tabular-nums">
                {i + 1}
              </span>
              <h3 className="font-mono text-sm text-foreground">{cls.name}</h3>
              <p className="text-sm leading-relaxed text-muted-foreground">{cls.body}</p>
            </div>
          ))}
        </div>

        <div className="mt-10 flex flex-col gap-4">
          <h3 className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
            Kill switches
          </h3>
          <ul className="flex flex-wrap gap-2">
            {KILL_SWITCHES.map((name) => (
              <li key={name}>
                <code className="inline-flex rounded-md border border-white/10 bg-black/40 px-2.5 py-1.5 font-mono text-xs text-foreground">
                  {name}
                </code>
              </li>
            ))}
          </ul>
          <p className="text-sm leading-relaxed text-muted-foreground">
            The full posture, with every claim cross-referenced to the source
            file that implements it, lives in{" "}
            <a
              href="https://github.com/agnelnieves/tolkin/blob/main/PRIVACY.md"
              rel="noopener"
              className="text-foreground underline decoration-white/30 underline-offset-4 transition-colors duration-150 ease-out hover:decoration-lime-300"
            >
              PRIVACY.md
            </a>
            .
          </p>
        </div>
      </div>
    </section>
  );
}
