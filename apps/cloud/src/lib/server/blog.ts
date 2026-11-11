import type { BlogPost, BlogPostMetadata } from "$lib/blog";

const modules = import.meta.glob<BlogPostMetadata>("/src/routes/blog/*/+page.svx", {
  eager: true,
  import: "metadata",
});

/** Every blog post, newest first. Server-only so article bodies stay out of client bundles. */
export const posts: BlogPost[] = Object.entries(modules)
  .map(([path, metadata]) => ({ ...metadata, slug: path.split("/").at(-2)! }))
  .sort((a, b) => b.published.localeCompare(a.published));
