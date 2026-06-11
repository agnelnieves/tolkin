import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  // Every route is static; exporting deploys plain files with no server
  // build, which sidesteps monorepo staging on the deploy host entirely.
  output: "export",
  trailingSlash: true,
};

export default nextConfig;
