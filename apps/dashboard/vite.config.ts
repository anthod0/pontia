import adapter from "@sveltejs/adapter-static";
import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { loadEnv, type ProxyOptions } from "vite";
import { configDefaults, defineConfig } from "vitest/config";

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const viteEnv = loadEnv(mode, process.cwd(), "");
  const dashboardMode =
    process.env.VITE_DASHBOARD_MODE?.trim() || viteEnv.VITE_DASHBOARD_MODE || "local";
  const env =
    dashboardMode === "public" ? { ...viteEnv, ...loadEnv("public", process.cwd(), "") } : viteEnv;
  if (dashboardMode !== "local" && dashboardMode !== "public") {
    throw new Error(
      `VITE_DASHBOARD_MODE must be "local" or "public", received ${JSON.stringify(dashboardMode)}`,
    );
  }

  const publicDashboard = dashboardMode === "public";
  const publicDevBackend = env.PONTIA_PUBLIC_DEV_BACKEND?.trim();
  const publicDevToken = env.PONTIA_EXTERNAL_API_TOKEN?.trim();
  const apiProxy: ProxyOptions | undefined = publicDashboard
    ? publicDevBackend
      ? {
          target: publicDevBackend,
          changeOrigin: true,
          configure(proxy) {
            if (!publicDevToken) return;
            proxy.on("proxyReq", (request) => {
              request.setHeader("Authorization", `Bearer ${publicDevToken}`);
            });
          },
        }
      : undefined
    : { target: "http://127.0.0.1:8080", changeOrigin: true };

  return {
    plugins: [
      tailwindcss(),
      sveltekit({
        preprocess: vitePreprocess(),
        files: {
          assets: "public",
        },
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
      }),
    ],
    define: {
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
      host: true,
      proxy: apiProxy ? { "/api": apiProxy } : undefined,
    },
    preview: {
      proxy: {},
    },
  };
});
