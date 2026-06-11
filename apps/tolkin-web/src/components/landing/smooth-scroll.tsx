"use client";

import Lenis from "lenis";
import { useEffect } from "react";

// Lenis smooth scrolling for the landing page only. Bails out entirely when
// the user prefers reduced motion: native scrolling is left untouched.
// `anchors: true` keeps in-page #links working through Lenis.
export function SmoothScroll() {
  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }

    const lenis = new Lenis({ anchors: true });
    let frame = 0;

    const raf = (time: number) => {
      lenis.raf(time);
      frame = requestAnimationFrame(raf);
    };
    frame = requestAnimationFrame(raf);

    return () => {
      cancelAnimationFrame(frame);
      lenis.destroy();
    };
  }, []);

  return null;
}
