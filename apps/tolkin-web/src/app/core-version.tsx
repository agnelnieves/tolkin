"use client";

import { useEffect, useState } from "react";

export function CoreVersion() {
  const [version, setVersion] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const mod = await import("tolkin-core-wasm");
        await mod.default();
        if (!cancelled) setVersion(mod.version());
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) return <span className="text-red-400">core load failed: {error}</span>;
  if (!version) return <span className="text-zinc-500">loading core...</span>;
  return <span className="text-zinc-400">core {version}</span>;
}
