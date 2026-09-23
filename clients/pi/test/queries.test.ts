import { realpath, symlink } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { loadProfileSystemPrompt } from "../src/profile.js";
import { isActiveRegisteredWorkspace } from "../src/workspace.js";
import { tempDir } from "./temp-dir.js";

test.each(["1", undefined])("loads the session's profile version %s over RPC", async (version) => {
  const request = vi.fn(async (method: string) => method === "session.get"
    ? { session: { execution_profile_id: "reviewer", execution_profile_version: version } }
    : { agent_profile: { system_prompt_template: "Review carefully" } });
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).resolves.toBe("Review carefully");
  expect(request.mock.calls).toEqual([
    ["session.get", { session_id: "sess_1" }],
    ["profile.get", { profile_id: "reviewer", ...(version ? { version } : {}) }],
  ]);
});

test("profile loading skips unbound sessions and sessions without a profile", async () => {
  const request = vi.fn(async () => ({ session: {} }));
  await expect(loadProfileSystemPrompt({ request })).resolves.toBeUndefined();
  expect(request).not.toHaveBeenCalled();
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).resolves.toBeUndefined();
  expect(request).toHaveBeenCalledOnce();
});

test.each([null, "", "   "])("ignores an empty profile prompt %j", async (prompt) => {
  const request = vi.fn(async (method: string) => method === "session.get"
    ? { session: { execution_profile_id: "reviewer" } }
    : { agent_profile: { system_prompt_template: prompt } });
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).resolves.toBeUndefined();
});

test("requires an active workspace whose canonical path matches cwd", async () => {
  const root = await tempDir("pi-workspace-");
  const canonical = await realpath(root);
  const link = join(root, "alias");
  await symlink(root, link);
  const request = vi.fn(async () => ({ workspaces: [
    { canonical_path: canonical, state: "active" },
    { canonical_path: join(root, "deleted"), state: "deleted" },
  ] }));
  await expect(isActiveRegisteredWorkspace({ request }, link)).resolves.toBe(true);
  await expect(isActiveRegisteredWorkspace({ request }, join(root, "deleted"))).resolves.toBe(false);
  await expect(isActiveRegisteredWorkspace({ request }, join(root, "unknown"))).resolves.toBe(false);
  await expect(isActiveRegisteredWorkspace({ request }, undefined)).resolves.toBe(false);
});

test("propagates RPC failures from profile and workspace queries", async () => {
  const root = await tempDir("pi-queries-");
  const request = async () => { throw new Error("disconnected"); };
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).rejects.toThrow("disconnected");
  await expect(isActiveRegisteredWorkspace({ request }, root)).rejects.toThrow("disconnected");
});
