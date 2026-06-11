"use client";

import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { SplitText } from "gsap/SplitText";

// Single registration point for the landing page's GSAP plugins. Every
// landing client component imports gsap from here so ScrollTrigger and
// SplitText are registered exactly once, before any tween is created.
gsap.registerPlugin(ScrollTrigger, SplitText);

// Reduced-motion media query string, shared so every matchMedia block on the
// landing gates motion on the exact same condition.
export const MOTION_OK = "(prefers-reduced-motion: no-preference)";

let refreshScheduled = false;

// Several components split text after document.fonts.ready resolves, all in
// the same tick. Each asks for a ScrollTrigger refresh; this coalesces the
// requests into one layout pass on the next animation frame.
export function scheduleScrollTriggerRefresh(): void {
  if (refreshScheduled) return;
  refreshScheduled = true;
  requestAnimationFrame(() => {
    refreshScheduled = false;
    ScrollTrigger.refresh();
  });
}

export { gsap, ScrollTrigger, SplitText };
