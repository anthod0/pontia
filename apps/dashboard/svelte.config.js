import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { loadEnv } from "vite";

const modeFlag = process.argv.findIndex((argument) => argument === "--mode");
const inlineMode = process.argv.find((argument) => argument.startsWith("--mode="))?.slice(7);
const viteMode =
  inlineMode ??
  (modeFlag >= 0 ? process.argv[modeFlag + 1] : undefined) ??
  (process.argv.some((argument) => argument === "build" || argument === "preview")
    ? "production"
    : "development");
const dashboardMode = loadEnv(viteMode, process.cwd(), "VITE_").VITE_DASHBOARD_MODE ?? "local";
if (dashboardMode !== "local" && dashboardMode !== "public") {
  throw new Error(
    `VITE_DASHBOARD_MODE must be "local" or "public", received ${JSON.stringify(dashboardMode)}`,
  );
}

const publicDashboard = dashboardMode === "public";

/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    alias: {
      "$dashboard-mode": publicDashboard ? "src/modes/public" : "src/modes/local",
    },
    adapter: adapter({
      pages: "dist",
      assets: "dist",
      fallback: "index.html",
    }),
    paths: {
      base: publicDashboard ? "" : "/dashboard",
    },
  },
};
