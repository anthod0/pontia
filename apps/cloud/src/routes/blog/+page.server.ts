import { posts } from "$lib/server/blog";
import { pageLocale } from "$lib/page-locale";
import type { PageServerLoad } from "./$types";

export const load: PageServerLoad = ({ url }) => ({ posts: posts(pageLocale(url)) });
