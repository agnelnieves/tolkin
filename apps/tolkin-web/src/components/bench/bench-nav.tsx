import Link from "next/link";

const LINKS = [
  { href: "/", label: "Home" },
  { href: "/analyzer/", label: "Analyzer" },
  {
    href: "https://github.com/agnelnieves/tolkin",
    label: "GitHub",
    external: true,
  },
];

export function BenchNav() {
  return (
    <header className="sticky top-0 z-50 border-b border-white/10 bg-[#0c0c0b]/80 backdrop-blur-md">
      <nav
        aria-label="Bench page navigation"
        className="mx-auto flex h-14 max-w-[1200px] items-center justify-between gap-4 px-6"
      >
        <Link
          href="/"
          className="font-display text-lg font-semibold tracking-tight text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-lime-400/60 focus-visible:ring-offset-2 focus-visible:ring-offset-[#0c0c0b]"
        >
          tolkin
        </Link>

        <ul className="flex items-center gap-1" role="list">
          {LINKS.map((link) => (
            <li key={link.label}>
              {link.external ? (
                <a
                  href={link.href}
                  rel="noopener noreferrer"
                  className="flex min-h-11 items-center rounded-md px-3 text-sm text-zinc-400 transition-colors duration-150 hover:text-zinc-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-lime-400/60"
                >
                  {link.label}
                </a>
              ) : (
                <Link
                  href={link.href}
                  className="flex min-h-11 items-center rounded-md px-3 text-sm text-zinc-400 transition-colors duration-150 hover:text-zinc-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-lime-400/60"
                >
                  {link.label}
                </Link>
              )}
            </li>
          ))}
        </ul>
      </nav>
    </header>
  );
}
