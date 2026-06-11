import { cn } from "@/lib/utils";

// Decorative 1px section divider. Server-rendered at full width so no-JS,
// reduced motion, and crawlers see a normal hairline; LandingMotion picks up
// `data-hairline` and draws it in (scaleX 0 to 1, origin left) on first
// viewport entry. Lives at the bottom edge of a `relative` section, replacing
// what used to be a border-b on the section box.
export function Hairline({ className }: { className?: string }) {
  return (
    <div
      aria-hidden="true"
      data-hairline
      className={cn("absolute inset-x-0 bottom-0 h-px bg-white/10", className)}
    />
  );
}
