"use client";

// Hero background: the FaultyTerminal shader behind a glow base, a text-side
// scrim, and the edge vignette. Mount and run rules match the rest of the
// landing motion: nothing mounts under reduced motion (the glow gradient is
// the fallback), the shader chunk loads at idle, and the render loop pauses
// whenever the hero leaves the viewport or the tab is hidden.

import dynamic from "next/dynamic";
import { useEffect, useRef, useState } from "react";

const FaultyTerminal = dynamic(() => import("./faulty-terminal"), { ssr: false });

export default function HeroField() {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [ready, setReady] = useState(false);
  const [finePointer, setFinePointer] = useState(false);
  const [inView, setInView] = useState(true);
  const [tabVisible, setTabVisible] = useState(true);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    setFinePointer(window.matchMedia("(pointer: fine)").matches);

    let fallback: ReturnType<typeof setTimeout> | undefined;
    let idleId = 0;
    const arm = () => setReady(true);
    if ("requestIdleCallback" in window) {
      idleId = window.requestIdleCallback(arm, { timeout: 2500 });
    } else {
      fallback = setTimeout(arm, 350);
    }

    const wrap = wrapRef.current;
    const io = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), {
      rootMargin: "80px",
    });
    if (wrap) io.observe(wrap);

    const onVisibility = () => setTabVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      if (idleId && "cancelIdleCallback" in window) window.cancelIdleCallback(idleId);
      if (fallback) clearTimeout(fallback);
      io.disconnect();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  return (
    <div ref={wrapRef} aria-hidden className="pointer-events-none absolute inset-0 overflow-hidden">
      <div className="hero-field-glow absolute inset-0" />
      {ready ? (
        <FaultyTerminal
          className="absolute inset-0"
          scale={1.6}
          gridMul={[2, 1]}
          digitSize={1.3}
          timeScale={0.45}
          pause={!(inView && tabVisible)}
          scanlineIntensity={0.4}
          glitchAmount={0.8}
          flickerAmount={0.6}
          noiseAmp={0.9}
          chromaticAberration={0}
          dither={0}
          curvature={0.12}
          tint="#aee061"
          mouseReact={finePointer}
          mouseStrength={0.12}
          pageLoadAnimation
          brightness={0.34}
        />
      ) : null}
      <div className="hero-text-scrim absolute inset-0" />
      <div className="hero-field-vignette absolute inset-0" />
    </div>
  );
}
