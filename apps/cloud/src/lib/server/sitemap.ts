import { locales, localizeHref } from "$lib/paraglide/runtime";
import type { BlogPost } from "$lib/blog";
import { siteOrigin as origin } from "$lib/site";

function entry(path: string, lastmod?: string): string {
  const alternates = [
    ...locales.map(
      (locale) =>
        `<xhtml:link rel="alternate" hreflang="${locale}" href="${origin}${localizeHref(path, { locale })}"/>`,
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

export function renderSitemap(docSlugs: string[], posts: BlogPost[]): string {
  const urls = [
    entry("/"),
    ...docSlugs.toSorted().map((slug) => entry(slug ? `/docs/${slug}` : "/docs")),
    entry("/blog"),
    ...posts.map((post) => entry(`/blog/${post.slug}`, post.updated ?? post.published)),
  ];
  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls.join("\n")}
</urlset>
`;
}
