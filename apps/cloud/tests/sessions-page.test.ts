import { expect, test } from "bun:test";
import { mkdtempDisposable } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { createServer } from "vite";

test("the sessions page renders a revoke form for each browser and CLI login", async () => {
  await using cache = await mkdtempDisposable(join(tmpdir(), "pontia-sessions-page-"));
  const server = await createServer({
    configFile: false,
    cacheDir: cache.path,
    plugins: [svelte({ configFile: false })],
    resolve: { alias: { $lib: fileURLToPath(new URL("../src/lib", import.meta.url)) } },
    server: { middlewareMode: true, hmr: false, watch: null },
  });
  try {
    const { default: Page } = await server.ssrLoadModule(
      "/src/routes/settings/sessions/+page.svelte",
    );
    const { render } = await server.ssrLoadModule("svelte/server");
    const sessions = [
      { id: "current-browser", kind: "browser", current: true },
      { id: "cli-credential", kind: "cli", current: false },
    ];
    const { body } = render(Page, { props: { data: { sessions } } });
    const targets: (string | null)[] = [];
    await new HTMLRewriter()
      .on('form[method="POST"][action="?/revokeSession"] input[name="session_id"]', {
        element(input) {
          targets.push(input.getAttribute("value"));
        },
      })
      .transform(new Response(body))
      .text();
    expect(targets).toEqual(["current-browser", "cli-credential"]);
  } finally {
    await server.close();
  }
}, 30_000);
