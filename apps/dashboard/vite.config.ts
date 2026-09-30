import adapter from "@sveltejs/adapter-static";
import tailwindcss from "@tailwindcss/vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { loadEnv, type ProxyOptions } from "vite";
import { defineConfig } from "vitest/config";

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
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
  const publicDevBridge = command === "serve" && mode === "development" && publicDashboard;

  if (publicDevBridge && (!publicDevBackend || !publicDevToken)) {
    throw new Error(
      "Public Dashboard development requires both PONTIA_PUBLIC_DEV_BACKEND and PONTIA_EXTERNAL_API_TOKEN",
    );
  }

  if (publicDevBridge) {
    const backend = new URL(publicDevBackend!);
    if (
      !["http:", "https:"].includes(backend.protocol) ||
      backend.username ||
      backend.password ||
      backend.pathname !== "/" ||
      backend.search ||
      backend.hash
    ) {
      throw new Error(
        "PONTIA_PUBLIC_DEV_BACKEND must be an HTTP(S) origin without credentials or a path",
      );
    }
  }

  const apiProxy: ProxyOptions = {
    target: publicDevBridge ? publicDevBackend! : "http://127.0.0.1:8080",
    changeOrigin: true,
    configure(proxy) {
      if (!publicDevBridge) return;
      proxy.on("proxyReq", (request) => {
        request.setHeader("Authorization", `Bearer ${publicDevToken}`);
      });
    },
  };

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
    },
    server: {
      host: true,
      proxy: {
        "/api": apiProxy,
      },
    },
  };
});
