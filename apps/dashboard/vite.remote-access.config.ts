import { defineConfig } from "vite";

// Builds the Public Dashboard remote access module separately from the SPA. The SPA loads
// `/_remote-access/remote-access.js` at runtime, so this must run after the SPA build.
export default defineConfig({
  base: "/_remote-access/",
  publicDir: false,
  build: {
    outDir: "dist/_remote-access",
    emptyOutDir: true,
    rolldownOptions: {
      input: "src/modes/public/remote-access/index.ts",
      preserveEntrySignatures: "strict",
      output: {
        entryFileNames: "remote-access.js",
      },
    },
  },
});
