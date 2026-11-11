import { posts } from "$lib/server/blog";

export const prerender = true;

const origin = "https://pontia.dev";

const docs = Object.keys(import.meta.glob("/src/routes/docs/**/+page.svx")).map((path) =>
  path.replace(/^\/src\/routes/, "").replace(/\/\+page\.svx$/, ""),
);

export function GET(): Response {
  const urls: { path: string; lastmod?: string }[] = [
    { path: "/" },
    ...docs.sort().map((path) => ({ path })),
    { path: "/blog" },
    ...posts.map((post) => ({
      path: `/blog/${post.slug}`,
      lastmod: post.updated ?? post.published,
    })),
  ];
  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls
  .map(({ path, lastmod }) =>
    lastmod
      ? `  <url><loc>${origin}${path}</loc><lastmod>${lastmod}</lastmod></url>`
      : `  <url><loc>${origin}${path}</loc></url>`,
  )
  .join("\n")}
</urlset>
`;
  return new Response(body, { headers: { "content-type": "application/xml" } });
}
