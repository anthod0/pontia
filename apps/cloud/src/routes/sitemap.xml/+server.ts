import { docSlugs } from "$lib/content";
import { hreflangs } from "$lib/locales";
import { locales, localizeHref } from "$lib/paraglide/runtime";
import { posts } from "$lib/server/blog";

export const prerender = true;

const origin = "https://pontia.dev";

function entry(path: string, lastmod?: string): string {
  const alternates = [
    ...locales.map(
      (locale) =>
        `<xhtml:link rel="alternate" hreflang="${hreflangs[locale]}" href="${origin}${localizeHref(path, { locale })}"/>`,
    ),
    `<xhtml:link rel="alternate" hreflang="x-default" href="${origin}${path}"/>`,
  ].join("");
  return locales
    .map((locale) => {
      const loc = `<loc>${origin}${localizeHref(path, { locale })}</loc>`;
      return `  <url>${loc}${lastmod ? `<lastmod>${lastmod}</lastmod>` : ""}${alternates}</url>`;
    })
    .join("\n");
}

export function GET(): Response {
  const urls = [
    entry("/"),
    ...docSlugs.sort().map((slug) => entry(slug ? `/docs/${slug}` : "/docs")),
    entry("/blog"),
    ...posts("en").map((post) => entry(`/blog/${post.slug}`, post.updated ?? post.published)),
  ];
  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls.join("\n")}
</urlset>
`;
  return new Response(body, { headers: { "content-type": "application/xml" } });
}
