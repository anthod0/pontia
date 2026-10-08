import tailwindcss from "@tailwindcss/vite";
import adapter from "@sveltejs/adapter-cloudflare";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";
import { mdsvex } from "mdsvex";

export default defineConfig({
  plugins: [
    tailwindcss(),
    sveltekit({
      extensions: [".svelte", ".svx"],
      preprocess: [mdsvex({ extensions: [".svx"] })],
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
