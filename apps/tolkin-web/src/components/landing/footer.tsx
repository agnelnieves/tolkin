import Link from "next/link";

const PRODUCT_LINKS = [
  { label: "Analyzer", href: "/analyzer" },
  { label: "Bench", href: "/bench" },
  { label: "GitHub", href: "https://github.com/agnelnieves/tolkin", external: true },
  { label: "npm", href: "https://www.npmjs.com/package/tolkin-cli", external: true },
] as const;

const TRUST_LINKS = [
  {
    label: "Privacy posture",
    href: "https://github.com/agnelnieves/tolkin/blob/main/PRIVACY.md",
    external: true,
  },
  { label: "Benchmark methodology", href: "/bench", external: false },
] as const;

function FooterLink({
  label,
  href,
  external,
}: {
  label: string;
  href: string;
  external?: boolean;
}) {
  const className =
    "text-sm text-muted-foreground transition-colors duration-150 ease-out hover:text-foreground";
  if (external) {
    return (
      <a href={href} rel="noopener" className={className}>
        {label}
      </a>
    );
  }
  return (
    <Link href={href} className={className}>
      {label}
    </Link>
  );
}

export function Footer() {
  return (
    <footer className="mx-auto max-w-[1200px] px-6 py-16">
      <div className="flex flex-col justify-between gap-12 md:flex-row">
        <div className="flex flex-col gap-3">
          <p className="font-display text-lg font-semibold tracking-tight">tolkin</p>
          <p className="max-w-[30ch] text-sm leading-relaxed text-muted-foreground">
            Built on a Rust core. Measured on real logs.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-x-16 gap-y-3">
          <div className="flex flex-col gap-3">
            <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
              Product
            </p>
            {PRODUCT_LINKS.map((link) => (
              <FooterLink key={link.label} {...link} />
            ))}
          </div>
          <div className="flex flex-col gap-3">
            <p className="font-mono text-[11px] uppercase tracking-[0.2em] text-muted-foreground">
              Trust
            </p>
            {TRUST_LINKS.map((link) => (
              <FooterLink key={link.label} {...link} />
            ))}
          </div>
        </div>
      </div>

      <p className="mt-14 border-t border-white/10 pt-6 font-mono text-xs text-muted-foreground">
        © 2026 Agnel Nieves
      </p>
    </footer>
  );
}
