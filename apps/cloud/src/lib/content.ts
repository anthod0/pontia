import type { Component } from "svelte";
import { baseLocale, type Locale } from "$lib/paraglide/runtime";
import { parseContentPath } from "$lib/content-path";

type Page = () => Promise<{ default: Component }>;

const docs = import.meta.glob<{ default: Component }>("/src/content/docs/*/**/*.svx");
const blog = import.meta.glob<{ default: Component }>("/src/content/blog/*/*.svx");

function baseSlugs(pages: Record<string, Page>): string[] {
  return Object.keys(pages).flatMap((path) => {
    const { locale, slug } = parseContentPath(path)!;
    return locale === baseLocale ? [slug] : [];
  });
}

/** Slug of each English documentation page; the overview is the empty slug. */
export const docSlugs = baseSlugs(docs).map((slug) => (slug === "index" ? "" : slug));

/** Slug of each English blog post. */
export const postSlugs = baseSlugs(blog);

export function docPage(locale: Locale, slug: string): Page | undefined {
  return docs[`/src/content/docs/${locale}/${slug || "index"}.svx`];
}

export function blogPost(locale: Locale, slug: string): Page | undefined {
  return blog[`/src/content/blog/${locale}/${slug}.svx`];
}
