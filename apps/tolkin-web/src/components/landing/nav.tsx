"use client";

import { Menu, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { CopyButton } from "./copy-button";

const BREW_COMMAND = "brew install agnelnieves/tolkin/tolkin";

const LINKS = [
  { href: "/#features", label: "Features" },
  { href: "/#local", label: "Local models" },
  { href: "/#privacy", label: "Privacy" },
  { href: "/bench", label: "Bench" },
  { href: "/analyzer", label: "Analyzer" },
  { href: "https://github.com/agnelnieves/tolkin", label: "GitHub", external: true },
];

// Duplicated-text hover slide: two stacked copies of the label inside an
// overflow-hidden span. On hover (hover-capable pointers only, motion
// allowed) the visible copy slides up and out while the duplicate slides in
// from below. Both layers render the same text at the same size, so the
// swap is layout-shift free; the duplicate is aria-hidden. CSS lives in
// globals.css under .nav-flip.
function NavFlipLabel({ label }: { label: string }) {
  return (
    <span className="nav-flip">
      <span className="nav-flip-top">{label}</span>
      <span aria-hidden="true" className="nav-flip-dup">
        {label}
      </span>
    </span>
  );
}

// Sticky top nav. Desktop: inline links plus a compact brew-install copy
// chip. Mobile: a simple disclosure list, 44px tap targets, no drawer.
export function Nav() {
  const [open, setOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 border-b border-white/10 bg-background/80 backdrop-blur-md">
      <nav
        aria-label="Main"
        className="mx-auto flex h-14 max-w-[1200px] items-center justify-between gap-4 px-6"
      >
        <Link
          href="/"
          className="font-display text-lg font-semibold tracking-tight text-foreground"
        >
          tolkin
        </Link>

        <div className="hidden items-center gap-1 md:flex">
          {LINKS.map((link) =>
            link.external ? (
              <a
                key={link.label}
                href={link.href}
                rel="noopener"
                className="nav-flip-trigger rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
              >
                <NavFlipLabel label={link.label} />
              </a>
            ) : (
              <Link
                key={link.label}
                href={link.href}
                className="nav-flip-trigger rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
              >
                <NavFlipLabel label={link.label} />
              </Link>
            ),
          )}
        </div>

        <div className="flex items-center gap-2">
          <div className="hidden items-center gap-0.5 rounded-lg border border-white/10 bg-white/[0.03] py-0.5 pl-3 sm:flex">
            <code className="font-mono text-xs text-muted-foreground">
              brew install <span className="text-foreground">tolkin</span>
            </code>
            <CopyButton
              text={BREW_COMMAND}
              label="Copy the Homebrew install command"
              className="size-9"
            />
          </div>

          <button
            type="button"
            aria-expanded={open}
            aria-controls="mobile-nav"
            aria-label={open ? "Close menu" : "Open menu"}
            onClick={() => setOpen((v) => !v)}
            className="inline-flex size-11 cursor-pointer items-center justify-center rounded-md text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground md:hidden"
          >
            {open ? (
              <X aria-hidden="true" className="size-5" />
            ) : (
              <Menu aria-hidden="true" className="size-5" />
            )}
          </button>
        </div>
      </nav>

      {open ? (
        <div id="mobile-nav" className="border-t border-white/10 px-4 pt-2 pb-4 md:hidden">
          <ul className="flex flex-col">
            {LINKS.map((link) => (
              <li key={link.label}>
                {link.external ? (
                  <a
                    href={link.href}
                    rel="noopener"
                    onClick={() => setOpen(false)}
                    className="flex min-h-11 items-center rounded-md px-3 text-sm text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
                  >
                    {link.label}
                  </a>
                ) : (
                  <Link
                    href={link.href}
                    onClick={() => setOpen(false)}
                    className="flex min-h-11 items-center rounded-md px-3 text-sm text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground"
                  >
                    {link.label}
                  </Link>
                )}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </header>
  );
}
