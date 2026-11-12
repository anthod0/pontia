import { expect, test } from "bun:test";
import { renderSitemap } from "../src/lib/server/sitemap";

test("publishes every language version with reciprocal alternates and an English default", () => {
  const sitemap = renderSitemap(
    ["setup", ""],
    [
      {
        slug: "news",
        title: "News",
        description: "News",
        published: "2026-10-01",
        updated: "2026-11-01",
      },
    ],
  );
  const entries = [...sitemap.matchAll(/<url>(.*?)<\/url>/g)].map((match) => match[1]);
  expect(entries).toHaveLength(20);
  for (const path of ["", "/docs", "/docs/setup", "/blog", "/blog/news"]) {
    const alternatives = [
      ["en", `https://pontia.dev${path || "/"}`],
      ["zh-Hans", `https://pontia.dev/zh${path}`],
      ["ja", `https://pontia.dev/ja${path}`],
      ["pt-BR", `https://pontia.dev/pt-br${path}`],
      ["x-default", `https://pontia.dev${path || "/"}`],
    ];
    for (const [, href] of alternatives.slice(0, 4)) {
      const entry = entries.find((entry) => entry.startsWith(`<loc>${href}</loc>`));
      expect(entry).toBeDefined();
      expect(
        [
          ...entry!.matchAll(/<xhtml:link rel="alternate" hreflang="([^"]+)" href="([^"]+)"\/>/g),
        ].map((match) => [match[1], match[2]]),
      ).toEqual(alternatives);
      if (path === "/blog/news") expect(entry).toContain("<lastmod>2026-11-01</lastmod>");
      else expect(entry).not.toContain("<lastmod>");
    }
  }
});

test("uses publication dates when posts have no update date and preserves the source slug order", () => {
  const slugs = ["setup", "", "configuration"];
  const sitemap = renderSitemap(slugs, [
    {
      slug: "news",
      title: "News",
      description: "News",
      published: "2026-10-01",
    },
  ]);
  expect(slugs).toEqual(["setup", "", "configuration"]);
  expect(sitemap.match(/<lastmod>2026-10-01<\/lastmod>/g)).toHaveLength(4);
});
