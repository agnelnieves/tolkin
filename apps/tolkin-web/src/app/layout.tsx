import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Tolkin",
  description: "Privacy-first AI token analyzer. Nothing leaves your browser.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
