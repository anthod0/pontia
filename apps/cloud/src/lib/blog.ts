import { getLocale } from "$lib/paraglide/runtime";

export interface BlogPostMetadata {
  title: string;
  description: string;
  /** ISO date, `YYYY-MM-DD`. */
  published: string;
  /** ISO date, `YYYY-MM-DD`, when the content changed after publication. */
  updated?: string;
}

export interface BlogPost extends BlogPostMetadata {
  slug: string;
}

export function formatDate(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString(getLocale(), {
    year: "numeric",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
  });
}
