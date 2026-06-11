"use client";

import { useLayoutEffect, useRef } from "react";
import { gsap, MOTION_OK } from "./motion";

// Page-level motion details that do not belong to any one section:
// 1. A 2px lime progress hairline fixed at the very top, scaleX scrubbed 1:1
//    against document scroll progress.
// 2. Every [data-hairline] section divider draws in (scaleX 0 to 1, origin
//    left, 0.6s) the first time it enters the viewport.
// All of it sits behind a prefers-reduced-motion gate; without motion the
// progress bar stays hidden (server-rendered at scaleX 0) and hairlines keep
// their full-width server state.
export function LandingMotion() {
  const bar = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const mm = gsap.matchMedia();

    mm.add(MOTION_OK, () => {
      if (bar.current) {
        gsap.fromTo(
          bar.current,
          { scaleX: 0 },
          {
            scaleX: 1,
            ease: "none",
            scrollTrigger: { start: 0, end: "max", scrub: true },
          },
        );
      }

      const lines = gsap.utils.toArray<HTMLElement>("[data-hairline]");
      for (const line of lines) {
        gsap.fromTo(
          line,
          { scaleX: 0, transformOrigin: "left center" },
          {
            scaleX: 1,
            duration: 0.6,
            ease: "power2.out",
            scrollTrigger: {
              trigger: line,
              start: "top 95%",
              once: true,
              toggleActions: "play none none none",
            },
          },
        );
      }
    });

    return () => mm.revert();
  }, []);

  return (
    <div
      ref={bar}
      aria-hidden="true"
      className="pointer-events-none fixed inset-x-0 top-0 z-[70] h-0.5 origin-left scale-x-0 bg-lime-300"
    />
  );
}
