"use client";

import { useEffect, useRef, useState } from "react";

const COMMAND = "tolkin project --json | jq .totals";

// The real shape of `tolkin project --json | jq .totals`, run on this repo.
const JSON_FIELDS = [
  { key: "files_scanned", value: "593" },
  { key: "context_files", value: "22" },
  { key: "context_tokens", value: "44174" },
  { key: "savings_min", value: "774" },
  { key: "savings_max", value: "2201" },
] as const;

const FINAL_LINE = "593 files measured. nothing left this machine.";

const PLAIN_TRANSCRIPT = [
  `$ ${COMMAND}`,
  "{",
  ...JSON_FIELDS.map((f, i) => `  "${f.key}": ${f.value}${i < JSON_FIELDS.length - 1 ? "," : ""}`),
  "}",
  FINAL_LINE,
].join("\n");

type Phase = { cmd: number; lines: number; final: boolean; done: boolean };

const FINISHED: Phase = {
  cmd: COMMAND.length,
  lines: JSON_FIELDS.length + 2,
  final: true,
  done: true,
};

// Typed terminal session. Types the command character by character, prints
// the JSON result line by line, then the closing lime line. Runs once, never
// loops. Under reduced motion the full transcript renders immediately. The
// animated region is aria-hidden; a static sr-only copy serves screen readers.
export function Terminal({ startDelay = 0 }: { startDelay?: number }) {
  const [phase, setPhase] = useState<Phase>({ cmd: 0, lines: 0, final: false, done: false });
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setPhase(FINISHED);
      return;
    }

    const queue = timers.current;
    const at = (ms: number, fn: () => void) => {
      queue.push(setTimeout(fn, ms));
    };

    let t = startDelay;
    for (let i = 1; i <= COMMAND.length; i++) {
      const chars = i;
      at(t, () => setPhase((p) => ({ ...p, cmd: chars })));
      t += 22;
    }
    t += 260;
    for (let i = 1; i <= JSON_FIELDS.length + 2; i++) {
      const lines = i;
      at(t, () => setPhase((p) => ({ ...p, lines })));
      t += 90;
    }
    t += 300;
    at(t, () => setPhase((p) => ({ ...p, final: true })));
    at(t + 200, () => setPhase(FINISHED));

    return () => {
      for (const handle of queue) clearTimeout(handle);
      queue.length = 0;
    };
  }, [startDelay]);

  const jsonLines: React.ReactNode[] = [];
  if (phase.lines >= 1) {
    jsonLines.push(<div key="open">{"{"}</div>);
  }
  JSON_FIELDS.forEach((field, i) => {
    if (phase.lines >= i + 2) {
      jsonLines.push(
        <div key={field.key}>
          {"  "}
          <span className="text-zinc-400">&quot;{field.key}&quot;</span>
          {": "}
          <span className="text-foreground tabular-nums">{field.value}</span>
          {i < JSON_FIELDS.length - 1 ? "," : ""}
        </div>,
      );
    }
  });
  if (phase.lines >= JSON_FIELDS.length + 2) {
    jsonLines.push(<div key="close">{"}"}</div>);
  }

  return (
    <div className="overflow-hidden rounded-xl border border-white/10 bg-black/55 shadow-[0_24px_80px_-32px_rgba(0,0,0,0.9)]">
      <div className="flex items-center gap-3 border-b border-white/10 px-4 py-3">
        <span aria-hidden="true" className="flex gap-1.5">
          <span className="size-2.5 rounded-full bg-white/15" />
          <span className="size-2.5 rounded-full bg-white/15" />
          <span className="size-2.5 rounded-full bg-white/15" />
        </span>
        <span className="font-mono text-xs text-muted-foreground">tolkin</span>
      </div>

      <div
        aria-hidden="true"
        className="min-h-[15.5rem] px-5 py-4 font-mono text-[13px] leading-[1.7] text-zinc-500"
      >
        <div>
          <span className="text-zinc-500">$ </span>
          <span className="text-foreground">{COMMAND.slice(0, phase.cmd)}</span>
          {!phase.done && phase.lines === 0 ? <span className="terminal-caret" /> : null}
        </div>
        {jsonLines}
        {phase.final ? <div className="mt-2 text-lime-300">{FINAL_LINE}</div> : null}
        {phase.done ? (
          <div className="mt-2">
            <span>$ </span>
            <span className="terminal-caret" />
          </div>
        ) : null}
      </div>

      <pre className="sr-only">{PLAIN_TRANSCRIPT}</pre>
    </div>
  );
}
