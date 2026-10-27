import tailwindcss from "@tailwindcss/vite";
import adapter from "@sveltejs/adapter-cloudflare";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";
import { mdsvex } from "mdsvex";
import { generateLlms } from "./scripts/generate-llms";

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
          highlight: { alias: { sh: "bash" } },
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
