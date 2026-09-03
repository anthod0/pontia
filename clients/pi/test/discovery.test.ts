import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";
import { resolvePontiaConnection } from "../src/discovery.js";
import { tempDir } from "./temp-dir.js";

describe("resolvePontiaConnection", () => {
  test("reads config only from PONTIA_HOME", async () => {
    const root = await tempDir("pontia-pi-discovery-");
    const pontiaHome = join(root, "pontia-home");
    const decoyConfig = join(root, "elsewhere", "config.toml");
    await mkdir(pontiaHome, { recursive: true });
    await mkdir(join(root, "elsewhere"), { recursive: true });
    await writeFile(join(pontiaHome, "config.toml"), 'bind_addr = "127.0.0.1:8089"\nexternal_api_token = "pontia-home-token"\n');
    await writeFile(decoyConfig, 'bind_addr = "127.0.0.1:18080"\nexternal_api_token = "decoy-token"\n');

    const fetchImpl = vi.fn();
    const result = await resolvePontiaConnection({
      env: { PONTIA_HOME: pontiaHome },
      fetch: fetchImpl as any,
    });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(result).toEqual({
      baseUrl: "http://127.0.0.1:8089",
      internalEventUrl: "http://127.0.0.1:8089/internal/v1/events",
      bindingUpsertUrl: "http://127.0.0.1:8089/internal/v1/runtime-bindings/upsert",
      externalApiUrl: "http://127.0.0.1:8089/external/v1",
      externalApiToken: "pontia-home-token",
    });
  });

  test("uses the daemon default when bind_addr is absent", async () => {
    const pontiaHome = await tempDir("pontia-pi-discovery-default-");
    await writeFile(join(pontiaHome, "config.toml"), 'external_api_token = "token"\n');

    const result = await resolvePontiaConnection({ pontiaHome });

    expect(result).toEqual({
      baseUrl: "http://127.0.0.1:8080",
      internalEventUrl: "http://127.0.0.1:8080/internal/v1/events",
      bindingUpsertUrl: "http://127.0.0.1:8080/internal/v1/runtime-bindings/upsert",
      externalApiUrl: "http://127.0.0.1:8080/external/v1",
      externalApiToken: "token",
    });
  });

  test("does not replace an explicitly invalid bind_addr with the default", async () => {
    const pontiaHome = await tempDir("pontia-pi-discovery-invalid-bind-");
    await writeFile(join(pontiaHome, "config.toml"), 'bind_addr = ""\nexternal_api_token = "token"\n');

    await expect(resolvePontiaConnection({ pontiaHome })).resolves.toBeUndefined();
  });

  test.each([
    ["empty", "   "],
    ["relative", "relative/pontia-home"],
    ["tilde-prefixed", "~/.pontia"],
  ])("stays inactive when PONTIA_HOME is %s", async (_case, pontiaHome) => {
    const result = await resolvePontiaConnection({ env: { PONTIA_HOME: pontiaHome } });

    expect(result).toBeUndefined();
  });
});
