import { error } from "@sveltejs/kit";
import { docPage, docSlugs } from "$lib/content";
import { pageLocale } from "$lib/page-locale";
import type { EntryGenerator, PageLoad } from "./$types";

export const entries: EntryGenerator = () => docSlugs.map((slug) => ({ slug }));

export const load: PageLoad = async ({ params, url }) => {
  const page = docPage(pageLocale(url), params.slug);
  if (!page) error(404, "Not found");
  return { content: (await page()).default };
};
