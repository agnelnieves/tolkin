import { cn } from "@/lib/utils";
import { FadeIn, RevealText } from "./reveal-text";

type SectionHeadingProps = {
  eyebrow: string;
  title: string;
  lede?: string;
  className?: string;
};

// Shared section opener: uppercase mono eyebrow, display heading, optional
// lede. Every landing section is labeled by one of these h2s. The heading
// gets the masked line reveal; eyebrow and lede get the soft lead-in fade.
export function SectionHeading({ eyebrow, title, lede, className }: SectionHeadingProps) {
  return (
    <div className={cn("max-w-3xl", className)}>
      <FadeIn
        as="p"
        className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground"
      >
        {eyebrow}
      </FadeIn>
      <RevealText
        as="h2"
        className="mt-4 font-display text-3xl font-semibold tracking-tight text-balance sm:text-4xl md:text-5xl"
      >
        {title}
      </RevealText>
      {lede ? (
        <FadeIn
          as="p"
          delay={0.1}
          className="mt-5 max-w-2xl text-base leading-relaxed text-muted-foreground sm:text-lg"
        >
          {lede}
        </FadeIn>
      ) : null}
    </div>
  );
}
