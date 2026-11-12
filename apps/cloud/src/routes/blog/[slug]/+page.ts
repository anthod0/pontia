import { error } from "@sveltejs/kit";
import { blogPost, postSlugs } from "$lib/content";
import { pageLocale } from "$lib/page-locale";
import type { EntryGenerator, PageLoad } from "./$types";

export const entries: EntryGenerator = () => postSlugs.map((slug) => ({ slug }));

export const load: PageLoad = async ({ params, url }) => {
  const post = blogPost(pageLocale(url), params.slug);
  if (!post) error(404, "Not found");
  return { content: (await post()).default };
};
