import adapter from "@sveltejs/adapter-static";
import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { loadEnv, searchForWorkspaceRoot } from "vite";
import { configDefaults, defineConfig } from "vitest/config";
import { mockApi } from "./mock/vitePlugin.ts";

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const viteEnv = loadEnv(mode, process.cwd(), "");
  const dashboardMode =
    process.env.VITE_DASHBOARD_MODE?.trim() || viteEnv.VITE_DASHBOARD_MODE || "local";
  if (dashboardMode !== "local" && dashboardMode !== "public") {
    throw new Error(
      `VITE_DASHBOARD_MODE must be "local" or "public", received ${JSON.stringify(dashboardMode)}`,
    );
  }

  const publicDashboard = dashboardMode === "public";
  // `just dev` points the Local Dashboard at a running pontiad; otherwise the mock API serves it.
  const devBackend = publicDashboard ? undefined : process.env.PONTIA_DEV_BACKEND?.trim();

  const backendUrl = devBackend ? new URL(devBackend) : undefined;

  return {
    plugins: [
      ...(process.env.VITEST || devBackend ? [] : [mockApi({ publicDashboard })]),
      tailwindcss(),
      sveltekit({
        preprocess: vitePreprocess(),
        files: {
          assets: "public",
        },
        alias: {
          $assets: "../../assets",
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
        typescript: {
          config(config) {
            config.include.push("../mock/**/*.ts");
          },
        },
      }),
    ],
    define: {
      "import.meta.env.VITE_DEV_BACKEND_PORT": JSON.stringify(
        backendUrl ? backendUrl.port || (backendUrl.protocol === "https:" ? "443" : "80") : "",
      ),
      "import.meta.env.VITE_DASHBOARD_MODE": JSON.stringify(dashboardMode),
    },
    resolve: {
      conditions: process.env.VITEST ? ["browser"] : undefined,
    },
    test: {
      environment: "jsdom",
      setupFiles: ["./tests/setup.ts"],
      include: publicDashboard ? ["tests/public/**/*.test.ts"] : ["tests/**/*.test.ts"],
      exclude: [...configDefaults.exclude, ...(publicDashboard ? [] : ["tests/public/**"])],
    },
    server: {
      fs: { allow: [searchForWorkspaceRoot(process.cwd()), "../../assets"] },
      host: true,
      proxy: devBackend ? { "/api": { target: devBackend, changeOrigin: true } } : undefined,
    },
    preview: {
      proxy: {},
    },
  };
});
