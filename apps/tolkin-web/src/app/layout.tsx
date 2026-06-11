import type { Metadata, Viewport } from "next";
import "./globals.css";
import { Bricolage_Grotesque, Geist, Geist_Mono } from "next/font/google";
import { Analytics } from "@/components/analytics";
import { cn } from "@/lib/utils";

const geist = Geist({ subsets: ["latin"], variable: "--font-sans" });
const geistMono = Geist_Mono({ subsets: ["latin"], variable: "--font-mono" });
const bricolage = Bricolage_Grotesque({
  subsets: ["latin"],
  variable: "--font-display",
});

export const metadata: Metadata = {
  metadataBase: new URL("https://tolkin.dev"),
  title: {
    template: "%s | Tolkin",
    default: "Tolkin: the privacy-first AI token analyzer",
  },
  description:
    "Tolkin measures the standing context of your AI agent setup: MCP servers, instruction files, skills, cache health. Deterministic numbers, zero egress.",
  alternates: {
    canonical: "/",
  },
  openGraph: {
    title: "Tolkin: the privacy-first AI token analyzer",
    description:
      "Tolkin measures the standing context of your AI agent setup: MCP servers, instruction files, skills, cache health. Deterministic numbers, zero egress.",
    url: "/",
    siteName: "Tolkin",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "Tolkin: the privacy-first AI token analyzer",
    description:
      "Tolkin measures the standing context of your AI agent setup: MCP servers, instruction files, skills, cache health. Deterministic numbers, zero egress.",
  },
};

export const viewport: Viewport = {
  themeColor: "#0c0c0b",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={cn(
        "font-sans",
        geist.variable,
        geistMono.variable,
        bricolage.variable,
      )}
    >
      <body>
        {children}
        {process.env.NODE_ENV === "production" ? <Analytics /> : null}
      </body>
    </html>
  );
}
