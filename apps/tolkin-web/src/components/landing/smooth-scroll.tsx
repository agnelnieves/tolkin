"use client";

import Lenis from "lenis";
import { useEffect } from "react";
import { gsap, ScrollTrigger } from "./motion";

// Lenis smooth scrolling for the landing page, wired to ScrollTrigger the
// canonical way: Lenis publishes scroll updates to ScrollTrigger, and the
// GSAP ticker is the single rAF driving Lenis (no second rAF loop, no
// double-smoothing, accurate trigger positions). Bails out entirely when the
// user prefers reduced motion: native scrolling is left untouched and
// ScrollTrigger reads the native scroll position directly.
// `anchors: true` keeps in-page #links working through Lenis.
export function SmoothScroll() {
  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }

    const lenis = new Lenis({ anchors: true });
    lenis.on("scroll", ScrollTrigger.update);

    const raf = (time: number) => {
      lenis.raf(time * 1000);
    };
    gsap.ticker.add(raf);
    gsap.ticker.lagSmoothing(0);

    return () => {
      gsap.ticker.remove(raf);
      lenis.destroy();
    };
  }, []);

  return null;
}
