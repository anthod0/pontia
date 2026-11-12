import { docSlugs } from "$lib/content";
import { baseLocale } from "$lib/paraglide/runtime";
import { posts } from "$lib/server/blog";
import { renderSitemap } from "$lib/server/sitemap";

export const prerender = true;

export function GET(): Response {
  return new Response(renderSitemap(docSlugs, posts(baseLocale)), {
    headers: { "content-type": "application/xml" },
  });
}
