import type { BlogPost, BlogPostMetadata } from "$lib/blog";
import type { Locale } from "$lib/paraglide/runtime";
import { parseContentPath } from "$lib/content-path";

const modules = import.meta.glob<BlogPostMetadata>("/src/content/blog/*/*.svx", {
  eager: true,
  import: "metadata",
});

/** Blog posts in `locale`, newest first. Server-only so article bodies stay out of client bundles. */
export function posts(locale: Locale): BlogPost[] {
  return Object.entries(modules)
    .flatMap(([path, metadata]) => {
      const { locale: postLocale, slug } = parseContentPath(path)!;
      return postLocale === locale ? [{ ...metadata, slug }] : [];
    })
    .sort((a, b) => b.published.localeCompare(a.published));
}
