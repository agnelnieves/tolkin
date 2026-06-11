import { codeToHtml } from "shiki";
import { CopyButton } from "./copy-button";

type CodeBlockProps = {
  code: string;
  lang: "bash" | "json";
  /** Filename or command label shown in the frame header. */
  label: string;
  /** Text placed on the clipboard; defaults to the full code. */
  copyText?: string;
};

// Server component: shiki runs at build time, no client highlighter ships.
// Poimandres reads lime-on-black; its own background is replaced with
// transparent so the frame owns the surface.
export async function CodeBlock({ code, lang, label, copyText }: CodeBlockProps) {
  const html = await codeToHtml(code, {
    lang,
    theme: "poimandres",
    colorReplacements: {
      "#1b1e28": "transparent",
    },
  });

  return (
    <figure className="code-block overflow-hidden rounded-xl border border-white/10 bg-black/40">
      <figcaption className="flex items-center justify-between gap-3 border-b border-white/10 px-4 py-1.5">
        <span className="flex items-center gap-3">
          <span aria-hidden="true" className="flex gap-1.5">
            <span className="size-2.5 rounded-full bg-white/15" />
            <span className="size-2.5 rounded-full bg-white/15" />
            <span className="size-2.5 rounded-full bg-white/15" />
          </span>
          <span className="font-mono text-xs text-muted-foreground">{label}</span>
        </span>
        <CopyButton text={copyText ?? code} label={`Copy ${label}`} />
      </figcaption>
      {/* shiki output is generated at build time from string literals in this repo, never from user input */}
      <div dangerouslySetInnerHTML={{ __html: html }} />
    </figure>
  );
}
