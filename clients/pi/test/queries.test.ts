import { realpath, symlink } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { loadSessionSystemPrompt } from "../src/instructions.js";
import { isActiveRegisteredWorkspace } from "../src/workspace.js";
import { tempDir } from "./temp-dir.js";

test("loads fixed Session instructions without querying mutable profiles", async () => {
  const request = vi.fn(async () => ({
    session_instructions: { system_prompt: "creation marker" },
  }));
  await expect(loadSessionSystemPrompt({ request }, "sess_1")).resolves.toBe("creation marker");
  expect(request.mock.calls).toEqual([["session.get", { session_id: "sess_1" }]]);
});

test("does not inject into unbound sessions or sessions without instructions", async () => {
  const request = vi.fn(async () => ({ session: {} }));
  await expect(loadSessionSystemPrompt({ request })).resolves.toBeUndefined();
  expect(request).not.toHaveBeenCalled();
  await expect(loadSessionSystemPrompt({ request }, "sess_1")).resolves.toBeUndefined();
  expect(request).toHaveBeenCalledOnce();
});

test("requires an active workspace whose canonical path matches cwd", async () => {
  const root = await tempDir("pi-workspace-");
  const canonical = await realpath(root);
  const link = join(root, "alias");
  await symlink(root, link);
  const request = vi.fn(async () => ({
    workspaces: [
      { canonical_path: canonical, state: "active" },
      { canonical_path: join(root, "deleted"), state: "deleted" },
    ],
  }));
  await expect(isActiveRegisteredWorkspace({ request }, link)).resolves.toBe(true);
  await expect(isActiveRegisteredWorkspace({ request }, join(root, "deleted"))).resolves.toBe(
    false,
  );
  await expect(isActiveRegisteredWorkspace({ request }, join(root, "unknown"))).resolves.toBe(
    false,
  );
  await expect(isActiveRegisteredWorkspace({ request }, undefined)).resolves.toBe(false);
});

test("propagates RPC failures from instruction and workspace queries", async () => {
  const root = await tempDir("pi-queries-");
  const request = async () => {
    throw new Error("disconnected");
  };
  await expect(loadSessionSystemPrompt({ request }, "sess_1")).rejects.toThrow("disconnected");
  await expect(isActiveRegisteredWorkspace({ request }, root)).rejects.toThrow("disconnected");
});
