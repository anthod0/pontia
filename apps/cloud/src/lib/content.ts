import type { Component } from "svelte";
import type { Locale } from "$lib/paraglide/runtime";

type Page = () => Promise<{ default: Component }>;

const docs = import.meta.glob<{ default: Component }>("/src/content/docs/*/**/*.svx");
const blog = import.meta.glob<{ default: Component }>("/src/content/blog/*/*.svx");

/** Slug of each English documentation page; the overview is the empty slug. */
export const docSlugs = Object.keys(docs)
  .flatMap((path) => path.match(/^\/src\/content\/docs\/en\/(.+)\.svx$/)?.[1] ?? [])
  .map((slug) => (slug === "index" ? "" : slug));

/** Slug of each English blog post. */
export const postSlugs = Object.keys(blog).flatMap(
  (path) => path.match(/^\/src\/content\/blog\/en\/(.+)\.svx$/)?.[1] ?? [],
);

export function docPage(locale: Locale, slug: string): Page | undefined {
  return docs[`/src/content/docs/${locale}/${slug || "index"}.svx`];
}

export function blogPost(locale: Locale, slug: string): Page | undefined {
  return blog[`/src/content/blog/${locale}/${slug}.svx`];
}
