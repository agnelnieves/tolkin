"use client";

import { type ElementType, type ReactNode, useLayoutEffect, useRef } from "react";
import { gsap, MOTION_OK, SplitText, scheduleScrollTriggerRefresh } from "./motion";

type RevealTextProps = {
  /** Rendered element, e.g. "h1", "h2", "h3". */
  as?: ElementType;
  className?: string;
  /** Extra delay in seconds, for choreographing sibling headings. */
  delay?: number;
  children: ReactNode;
};

// Masked line reveal for display headings. The element renders its full text
// on the server (SEO, no-JS, reduced motion all see plain text). With motion
// allowed, the text is split into lines after fonts load, each line clipped
// by an overflow-hidden mask and slid up from yPercent 100 when the heading
// scrolls into view. Plays once. SplitText's aria handling keeps the original
// text exposed to screen readers (aria-label on the element, aria-hidden on
// the split line divs).
export function RevealText({ as = "h2", className, delay = 0, children }: RevealTextProps) {
  const ref = useRef<HTMLElement>(null);
  const Tag = as;

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;

    const mm = gsap.matchMedia();

    mm.add(MOTION_OK, () => {
      let cancelled = false;
      let split: SplitText | null = null;
      let tween: gsap.core.Tween | null = null;

      // Hide before first paint so the unsplit text never flashes, then
      // split only after fonts settle so line boxes are final (zero CLS).
      gsap.set(el, { autoAlpha: 0 });

      document.fonts.ready.then(() => {
        if (cancelled) return;
        split = SplitText.create(el, {
          type: "lines",
          mask: "lines",
          autoSplit: true,
          aria: "auto",
          onSplit: (self) => {
            const t = gsap.from(self.lines, {
              yPercent: 100,
              duration: 0.7,
              ease: "power3.out",
              stagger: 0.08,
              delay,
              scrollTrigger: {
                trigger: el,
                start: "top 85%",
                once: true,
                toggleActions: "play none none none",
              },
            });
            tween = t;
            return t;
          },
        });
        gsap.set(el, { autoAlpha: 1 });
        scheduleScrollTriggerRefresh();
      });

      return () => {
        cancelled = true;
        tween?.scrollTrigger?.kill();
        tween?.kill();
        split?.revert();
        gsap.set(el, { clearProps: "opacity,visibility" });
      };
    });

    return () => mm.revert();
  }, [delay]);

  return (
    // biome-ignore lint/suspicious/noExplicitAny: polymorphic ref target
    <Tag ref={ref as any} className={className}>
      {children}
    </Tag>
  );
}

type FadeInProps = {
  as?: ElementType;
  className?: string;
  delay?: number;
  children: ReactNode;
};

// Soft entrance for section lead-in copy: one tween, opacity 0.001 to 1 with
// an 8px rise, played once at "top 88%". Deliberately not applied to body
// paragraphs in general; only ledes and eyebrows go through this.
export function FadeIn({ as = "p", className, delay = 0, children }: FadeInProps) {
  const ref = useRef<HTMLElement>(null);
  const Tag = as;

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;

    const mm = gsap.matchMedia();

    mm.add(MOTION_OK, () => {
      const tween = gsap.fromTo(
        el,
        { opacity: 0.001, y: 8 },
        {
          opacity: 1,
          y: 0,
          duration: 0.7,
          ease: "power2.out",
          delay,
          scrollTrigger: {
            trigger: el,
            start: "top 88%",
            once: true,
            toggleActions: "play none none none",
          },
        },
      );

      return () => {
        tween.scrollTrigger?.kill();
        tween.kill();
        gsap.set(el, { clearProps: "opacity,transform" });
      };
    });

    return () => mm.revert();
  }, [delay]);

  return (
    // biome-ignore lint/suspicious/noExplicitAny: polymorphic ref target
    <Tag ref={ref as any} className={className}>
      {children}
    </Tag>
  );
}
