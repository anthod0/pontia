import tailwindcss from "@tailwindcss/vite";
import adapter from "@sveltejs/adapter-cloudflare";
import { sveltekit } from "@sveltejs/kit/vite";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { mdsvex } from "mdsvex";
import { generateLlms } from "./scripts/generate-llms";
import { rehypeTableScroll } from "./scripts/rehype-table-scroll";

export default defineConfig({
  plugins: [
    {
      name: "generate-llms",
      apply: "build",
      async config() {
        await generateLlms();
      },
    },
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
          rehypePlugins: [rehypeTableScroll],
        }),
      ],
      compilerOptions: {
        // Force runes mode for the project, except for libraries. Can be removed in svelte 6.
        runes: ({ filename }) =>
          filename.split(/[/\\]/).includes("node_modules") ? undefined : true,
      },
      adapter: adapter(),
      csrf: {
        trustedOrigins: ["https://app.pontia.dev"],
      },
    }),
  ],
});
