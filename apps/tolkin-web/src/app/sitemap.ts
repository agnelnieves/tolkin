import type { MetadataRoute } from "next";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  const base = "https://tolkin.dev";
  return [
    {
      url: `${base}/`,
      changeFrequency: "weekly",
      priority: 1,
    },
    {
      url: `${base}/analyzer`,
      changeFrequency: "weekly",
      priority: 0.8,
    },
    {
      url: `${base}/bench`,
      changeFrequency: "weekly",
      priority: 0.7,
    },
  ];
}
