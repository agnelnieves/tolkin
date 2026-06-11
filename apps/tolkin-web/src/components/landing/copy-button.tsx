"use client";

import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

type CopyButtonProps = {
  text: string;
  label: string;
  className?: string;
};

// Icon button that copies `text` and flips to a check for 1.2s. The flip is
// opacity-only (both icons stacked), so nothing resizes. 44px hit target.
export function CopyButton({ text, label, className }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard unavailable (permissions, http). Stay quiet; the text is
      // selectable next to the button.
    }
  };

  return (
    <button
      type="button"
      aria-label={label}
      onClick={onCopy}
      className={cn(
        "relative inline-flex size-11 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground",
        className,
      )}
    >
      <Copy
        aria-hidden="true"
        className={cn(
          "absolute size-4 transition-opacity duration-150 ease-out",
          copied ? "opacity-0" : "opacity-100",
        )}
      />
      <Check
        aria-hidden="true"
        className={cn(
          "absolute size-4 text-lime-300 transition-opacity duration-150 ease-out",
          copied ? "opacity-100" : "opacity-0",
        )}
      />
      <span aria-live="polite" className="sr-only">
        {copied ? "Copied" : ""}
      </span>
    </button>
  );
}
