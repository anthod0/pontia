import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, test } from "vitest";

const root = new URL("..", import.meta.url).pathname;

async function readJson(path: string): Promise<any> {
  return JSON.parse(await readFile(join(root, path), "utf8"));
}

describe("Claude plugin package config", () => {
  test("declares a build script for the hook command target", async () => {
    const packageJson = await readJson("package.json");
    expect(packageJson.scripts.build).toBe("tsc -p tsconfig.build.json");
  });

  test("registers only phase 2 reporting hooks and does not register MessageDisplay", async () => {
    const hooksJson = await readJson("hooks/hooks.json");
    expect(Object.keys(hooksJson.hooks).sort()).toEqual([
      "SessionEnd",
      "SessionStart",
      "Stop",
      "StopFailure",
      "UserPromptSubmit",
    ]);
    for (const entries of Object.values(hooksJson.hooks) as any[]) {
      expect(entries[0].hooks[0]).toMatchObject({
        type: "command",
        command: "node",
        args: ["${CLAUDE_PLUGIN_ROOT}/dist/hook.js"],
      });
    }
  });
});
