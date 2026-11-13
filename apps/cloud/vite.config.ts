import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { mdsvex } from "mdsvex";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { generateLlms } from "./scripts/generate-llms";
import { localeInfo } from "./src/lib/locales";
import { rehypeLocalizeLinks } from "./scripts/rehype-localize-links";
import { rehypeTableScroll } from "./scripts/rehype-table-scroll";
import { localeAdapter } from "./scripts/locale-adapter";

export default defineConfig({
  plugins: [
    {
      name: "generate-llms",
      apply: "build",
      async config() {
        await generateLlms();
      },
    },
    paraglideVitePlugin({ project: "./project.inlang" }),
    tailwindcss(),
    sveltekit({
      extensions: [".svelte", ".svx"],
      preprocess: [
        mdsvex({
          extensions: [".svx"],
          // Applies to `.svx` files under a `blog` directory; docs pages have no layout.
          layout: {
            blog: fileURLToPath(new URL("./src/lib/components/BlogPost.svelte", import.meta.url)),
          },
          layoutPropForwarding: "runes",
          highlight: { alias: { sh: "bash" } },
          rehypePlugins: [rehypeTableScroll, rehypeLocalizeLinks],
        }),
      ],
      compilerOptions: {
        // Force runes mode for the project, except for libraries. Can be removed in svelte 6.
        runes: ({ filename }) =>
          filename.split(/[/\\]/).includes("node_modules") ? undefined : true,
      },
      adapter: localeAdapter(),
      // Localized pages are found by crawling from each locale's home page.
      prerender: {
        entries: ["*", ...Object.values(localeInfo).map(({ prefix }) => `/${prefix}` as const)],
      },
      csrf: {
        trustedOrigins: ["https://app.pontia.dev"],
      },
    }),
  ],
});
