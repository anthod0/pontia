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
  test("ignores pi settings config path and reads default pontia home config", async () => {
    const root = await tempDir();
    const settingsFile = join(root, ".pi", "agent", "settings.json");
    const settingsPontiaConfig = join(root, ".pontia-stable", "config.toml");
    const homePontiaConfig = join(root, ".pontia", "config.toml");
    await mkdir(join(root, ".pi", "agent"), { recursive: true });
    await mkdir(join(root, ".pontia-stable"), { recursive: true });
    await mkdir(join(root, ".pontia"), { recursive: true });
    await writeFile(settingsFile, JSON.stringify({ pontia: { config: settingsPontiaConfig } }));
    await writeFile(settingsPontiaConfig, 'bind_addr = "127.0.0.1:18080"\nexternal_api_token = "stable-token"\n');
    await writeFile(homePontiaConfig, 'bind_addr = "127.0.0.1:18081"\nexternal_api_token = "home-token"\n');

    const fetchImpl = vi.fn();

    const result = await resolvePontiaConnection({
      env: { HOME: root },
      fetch: fetchImpl as any,
    });

    expect(fetchImpl).not.toHaveBeenCalled();

    expect(result).toEqual({
      baseUrl: "http://127.0.0.1:18081",
      internalEventUrl: "http://127.0.0.1:18081/internal/v1/events",
      bindingUpsertUrl: "http://127.0.0.1:18081/internal/v1/runtime-bindings/upsert",
      externalApiUrl: "http://127.0.0.1:18081/external/v1",
      externalApiToken: "home-token",
    });
  });

  test("reads default config from pontia home", async () => {
    const root = await tempDir();
    const pontiaConfig = join(root, ".pontia", "config.toml");
    await mkdir(join(root, ".pontia"), { recursive: true });
    await writeFile(pontiaConfig, 'bind_addr = "127.0.0.1:8088"\nexternal_api_token = "home-token"\n');

    const fetchImpl = vi.fn();

    const result = await resolvePontiaConnection({
      env: { HOME: root },
      fetch: fetchImpl as any,
    });

    expect(fetchImpl).not.toHaveBeenCalled();

    expect(result?.baseUrl).toBe("http://127.0.0.1:8088");
    expect(result?.externalApiToken).toBe("home-token");
  });

  test("uses PONTIA_HOME as the default config root", async () => {
    const root = await tempDir();
    const pontiaHome = join(root, "custom-pontia");
    const pontiaConfig = join(pontiaHome, "config.toml");
    await mkdir(pontiaHome, { recursive: true });
    await writeFile(pontiaConfig, 'bind_addr = "127.0.0.1:8089"\nexternal_api_token = "custom-home-token"\n');

    const fetchImpl = vi.fn();

    const result = await resolvePontiaConnection({
      env: { HOME: root, PONTIA_HOME: pontiaHome },
      fetch: fetchImpl as any,
    });

    expect(fetchImpl).not.toHaveBeenCalled();

    expect(result?.baseUrl).toBe("http://127.0.0.1:8089");
    expect(result?.externalApiToken).toBe("custom-home-token");
  });
});
