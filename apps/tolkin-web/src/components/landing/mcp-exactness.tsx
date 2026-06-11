import { CodeBlock } from "./code-block";
import { SectionHeading } from "./section-heading";

const PROBE_OUTPUT = `$ tolkin mcp mcp.json --probe internal-docs --yes

probing internal-docs over the standard MCP handshake
manifest captured, redacted, cached at .tolkin/mcp-manifests/internal-docs.json

basis: measured (probed manifest, captured 2026-06-11) (exact; supersedes the catalog estimate)`;

export function McpExactness() {
  return (
    <section className="border-b border-white/10">
      <div className="mx-auto max-w-[1200px] px-6 py-20 sm:py-28">
        <div className="grid grid-cols-1 items-start gap-10 lg:grid-cols-2 lg:gap-14">
          <div>
            <SectionHeading
              eyebrow="MCP exactness"
              title="Unknown MCP servers become measured, not guessed."
              lede="The catalog covers 22 known servers. For the internal one only your team runs, --probe speaks the real MCP handshake (initialize, then tools/list) to a server your own config already names, tokenizes the returned manifest, and caches it."
            />
            <p className="mt-8 text-sm leading-relaxed text-muted-foreground">
              Probe once, commit{" "}
              <code className="font-mono text-xs text-foreground">.tolkin/mcp-manifests</code>, and
              the whole team reports exact numbers. The cached manifest is date-stamped,
              secret-redacted, and flagged when it goes stale.
            </p>
          </div>

          <CodeBlock code={PROBE_OUTPUT} lang="bash" label="tolkin mcp --probe" />
        </div>
      </div>
    </section>
  );
}
