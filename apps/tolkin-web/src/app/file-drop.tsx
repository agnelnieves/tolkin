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
export function FileDrop({ onText }: { onText: (text: string) => void }) {
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
        className={
          dragging
            ? "flex cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-indigo-500/60 bg-indigo-500/5 px-4 py-6 text-center transition-colors"
            : "flex cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-zinc-700 bg-zinc-950 px-4 py-6 text-center transition-colors hover:border-zinc-600"
        }
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
        <span className="text-sm text-zinc-300">
          Drop a file or <span className="text-indigo-400">browse</span>
        </span>
        <span className="mt-1 text-xs text-zinc-500">
          PDF, DOCX, XLSX, Markdown, JSON, YAML, code. Parsed in your browser.
        </span>
      </label>

      <div className="min-h-5 text-xs" aria-live="polite">
        {state.status === "parsing" ? (
          <span className="inline-flex items-center gap-2 text-zinc-400">
            <Spinner />
            Parsing {state.name}...
          </span>
        ) : state.status === "done" ? (
          <span className="text-zinc-500">
            Loaded <span className="text-zinc-300">{state.name}</span>{" "}
            <span className="text-zinc-600">({KIND_LABEL[state.kind]})</span>
          </span>
        ) : state.status === "error" ? (
          <span className="text-red-400">
            Could not parse {state.name}: {state.message}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <span
      className="inline-block h-3 w-3 animate-spin rounded-full border border-zinc-600 border-t-zinc-300"
      aria-hidden="true"
    />
  );
}
