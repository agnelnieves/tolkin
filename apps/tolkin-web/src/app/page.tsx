import { Distribution } from "@/components/landing/distribution";
import { Features } from "@/components/landing/features";
import { Footer } from "@/components/landing/footer";
import { Hero } from "@/components/landing/hero";
import { LocalIntelligence } from "@/components/landing/local-intelligence";
import { McpExactness } from "@/components/landing/mcp-exactness";
import { Nav } from "@/components/landing/nav";
import { Privacy } from "@/components/landing/privacy";
import { SmoothScroll } from "@/components/landing/smooth-scroll";
import { StatsBand } from "@/components/landing/stats-band";
import { Tiers } from "@/components/landing/tiers";
import { Verification } from "@/components/landing/verification";

const JSON_LD = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "Tolkin",
  description:
    "Privacy-first AI token analyzer. Measures the standing context of AI agent setups: MCP servers, instruction files, skills, cache health. Deterministic numbers, zero network egress.",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "macOS, Linux, Windows",
  softwareVersion: "0.14.0",
  url: "https://tolkin.dev",
  sameAs: ["https://github.com/agnelnieves/tolkin"],
  author: {
    "@type": "Person",
    name: "Agnel Nieves",
  },
  offers: {
    "@type": "Offer",
    price: "0",
    priceCurrency: "USD",
  },
};

export default function LandingPage() {
  return (
    <div className="dark landing min-h-screen bg-background text-foreground antialiased selection:bg-lime-300 selection:text-black">
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: static JSON-LD object defined above, no user input
        dangerouslySetInnerHTML={{ __html: JSON.stringify(JSON_LD) }}
      />
      <SmoothScroll />

      <a
        href="#main"
        className="sr-only z-[60] rounded-md bg-lime-300 px-4 py-2 font-mono text-sm text-black focus:not-sr-only focus:fixed focus:top-3 focus:left-3"
      >
        Skip to content
      </a>

      <Nav />

      <main id="main">
        <Hero />
        <StatsBand />
        <Features />
        <Tiers />
        <LocalIntelligence />
        <McpExactness />
        <Verification />
        <Privacy />
        <Distribution />
      </main>

      <Footer />
    </div>
  );
}
