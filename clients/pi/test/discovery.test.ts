import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test, vi } from "vitest";
import { resolvePontiaConnection } from "../src/discovery.js";

const tmpDirs: string[] = [];

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempDir() {
  const dir = await mkdtemp(join(tmpdir(), "pontia-pi-discovery-"));
  tmpDirs.push(dir);
  return dir;
}

describe("resolvePontiaConnection", () => {
  test("reads pontia config path from pi settings and builds local API URLs", async () => {
    const root = await tempDir();
    const settingsFile = join(root, ".pi", "agent", "settings.json");
    const pontiaConfig = join(root, ".config", "pontia-stable", "config.toml");
    await mkdir(join(root, ".pi", "agent"), { recursive: true });
    await mkdir(join(root, ".config", "pontia-stable"), { recursive: true });
    await writeFile(settingsFile, JSON.stringify({ pontia: { config: pontiaConfig } }));
    await writeFile(pontiaConfig, 'bind_addr = "127.0.0.1:18080"\nexternal_api_token = "stable-token"\n');

    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://127.0.0.1:18080/healthz");
      return new Response("ok", { status: 200 });
    });

    const result = await resolvePontiaConnection({
      env: { HOME: root },
      fetch: fetchImpl as any,
    });

    expect(result).toEqual({
      baseUrl: "http://127.0.0.1:18080",
      internalEventUrl: "http://127.0.0.1:18080/internal/v1/events",
      bindingUpsertUrl: "http://127.0.0.1:18080/internal/v1/runtime-bindings/upsert",
      externalApiUrl: "http://127.0.0.1:18080/external/v1",
      externalApiToken: "stable-token",
    });
  });
});
