import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { describe, expect, test } from "vitest";
const root = new URL("..", import.meta.url).pathname;
async function readJson(path) {
    return JSON.parse(await readFile(join(root, path), "utf8"));
}
describe("Claude plugin package config", () => {
    test("runs directly as executable JavaScript without a build step", async () => {
        const packageJson = await readJson("package.json");
        const hookPath = join(root, "src/hook.js");
        const hookSource = await readFile(hookPath, "utf8");
        const hookStat = await stat(hookPath);
        expect(packageJson.scripts).toEqual({ test: "vitest run" });
        expect(hookSource.startsWith("#!/usr/bin/env node\n")).toBe(true);
        expect(hookStat.mode & 0o111).not.toBe(0);
    });
    test("registers lifecycle hooks without intermediate message hooks", async () => {
        const hooksJson = await readJson("hooks/hooks.json");
        expect(Object.keys(hooksJson.hooks).sort()).toEqual([
            "PermissionRequest",
            "SessionEnd",
            "SessionStart",
            "Stop",
            "StopFailure",
            "UserPromptSubmit",
        ]);
        for (const entries of Object.values(hooksJson.hooks)) {
            expect(entries[0].hooks[0]).toEqual({
                type: "command",
                command: "${CLAUDE_PLUGIN_ROOT}/src/hook.js",
            });
        }
    });
});
