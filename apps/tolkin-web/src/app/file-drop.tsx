"use client";

import { useCallback, useId, useRef, useState } from "react";
import { type ParsedKind, parseFile } from "../lib/parse";

const KIND_LABEL: Record<ParsedKind, string> = {
  text: "text",
  pdf: "PDF",
  docx: "DOCX",
  xlsx: "XLSX",
};

type DropState =
  | { status: "idle" }
  | { status: "parsing"; name: string }
  | { status: "done"; name: string; kind: ParsedKind }
  | { status: "error"; name: string; message: string };

// Drag-and-drop plus click-to-pick dropzone. The selected file is read fully in
// memory and its extracted text is handed to the parent via onText, which feeds
// it into the shared textarea. Every parser is lazy-loaded inside parseFile.
// Nothing is uploaded.
//
// The dropzone has two presentations: a big first-run state when there is no
// content on screen, and a compact one-line bar once content is in the
// textarea. The compact mode is what the analyzer asks for once the user has
// pasted or loaded something; it keeps the affordance reachable without
// dominating the column.
export function FileDrop({
  onText,
  compact = false,
}: {
  onText: (text: string) => void;
  compact?: boolean;
}) {
  const [state, setState] = useState<DropState>({ status: "idle" });
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const inputId = useId();
  // Guards against an async parse from an earlier file overwriting a newer one.
  const runRef = useRef(0);

  const handleFile = useCallback(
    async (file: File) => {
      const runId = ++runRef.current;
      setState({ status: "parsing", name: file.name });
      try {
        const { kind, text } = await parseFile(file);
        if (runRef.current !== runId) return;
        onText(text);
        setState({ status: "done", name: file.name, kind });
      } catch (e) {
        if (runRef.current !== runId) return;
        setState({
          status: "error",
          name: file.name,
          message: e instanceof Error ? e.message : String(e),
        });
      }
    },
    [onText],
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) void handleFile(file);
    },
    [handleFile],
  );

  const base =
    "group/drop block w-full cursor-pointer rounded-lg border border-dashed bg-black/30 text-left transition-colors duration-150 ease-out focus-within:border-lime-300/60 hover:[@media(hover:hover)]:border-white/25";
  const tone = dragging
    ? "border-lime-300/60 bg-lime-300/5"
    : "border-white/15";
  const size = compact ? "px-4 py-3" : "min-h-32 px-4 py-6";

  return (
    <div className="space-y-2">
      <label
        htmlFor={inputId}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
        className={`${base} ${tone} ${size}`}
      >
        <input
          ref={inputRef}
          id={inputId}
          type="file"
          className="sr-only"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void handleFile(file);
            // Allow re-selecting the same file name to re-trigger onChange.
            e.target.value = "";
          }}
        />
        {compact ? (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <span className="font-mono text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
              Drop a file
              <span className="text-muted-foreground/60"> or </span>
              <span className="text-lime-300/90">browse</span>
            </span>
            <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/70">
              PDF, DOCX, XLSX, Markdown, JSON, YAML, code
            </span>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center gap-2 text-center">
            <span className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
              Drop a file
            </span>
            <span className="text-base text-foreground sm:text-sm">
              <span className="text-muted-foreground/80">or </span>
              <span className="text-lime-300/90 underline decoration-lime-300/40 underline-offset-4">
                browse
              </span>
            </span>
            <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-muted-foreground/70">
              PDF, DOCX, XLSX, Markdown, JSON, YAML, code. Parsed in your browser.
            </span>
          </div>
        )}
      </label>

      <div className="min-h-5 px-1 font-mono text-[11px]" aria-live="polite">
        {state.status === "parsing" ? (
          <span className="inline-flex items-center gap-2 uppercase tracking-[0.16em] text-muted-foreground">
            <Spinner />
            Parsing {state.name}
          </span>
        ) : state.status === "done" ? (
          <span className="uppercase tracking-[0.16em] text-muted-foreground">
            Loaded <span className="text-foreground">{state.name}</span>{" "}
            <span className="text-muted-foreground/70">({KIND_LABEL[state.kind]})</span>
          </span>
        ) : state.status === "error" ? (
          <span className="uppercase tracking-[0.16em] text-destructive">
            Could not parse {state.name}. {state.message}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <span
      className="inline-block h-3 w-3 motion-safe:animate-spin rounded-full border border-white/20 border-t-foreground"
      aria-hidden="true"
    />
  );
}
