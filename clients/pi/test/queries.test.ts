import { realpath, symlink } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { loadProfileSystemPrompt } from "../src/profile.js";
import { isActiveRegisteredWorkspace } from "../src/workspace.js";
import { tempDir } from "./temp-dir.js";

test("loads fixed Session instructions without querying mutable profiles", async () => {
  const request = vi.fn(async () => ({
    execution_profile_binding: { system_prompt: "creation marker" },
  }));
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).resolves.toBe("creation marker");
  expect(request.mock.calls).toEqual([["session.get", { session_id: "sess_1" }]]);
});

test("does not inject into unbound or legacy sessions", async () => {
  const request = vi.fn(async () => ({ session: { execution_profile_id: "reviewer" } }));
  await expect(loadProfileSystemPrompt({ request })).resolves.toBeUndefined();
  expect(request).not.toHaveBeenCalled();
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).resolves.toBeUndefined();
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

test("propagates RPC failures from profile and workspace queries", async () => {
  const root = await tempDir("pi-queries-");
  const request = async () => {
    throw new Error("disconnected");
  };
  await expect(loadProfileSystemPrompt({ request }, "sess_1")).rejects.toThrow("disconnected");
  await expect(isActiveRegisteredWorkspace({ request }, root)).rejects.toThrow("disconnected");
});

test("legacy sessions retain only their last effective native Profile section", async () => {
  const request = vi.fn(async (_method: string) => ({ session: {} }));
  const messages = [
    { role: "system", sections: { pontia_execution_profile: "original" } },
    { role: "system", sections: { other: "unrelated" } },
    { role: "system", sections: { pontia_execution_profile: "fixed" } },
  ];
  await expect(loadProfileSystemPrompt({ request }, "legacy", messages)).resolves.toBe("fixed");
  await expect(
    loadProfileSystemPrompt({ request }, "legacy", [
      ...messages,
      { role: "system", sections: { pontia_execution_profile: null } },
    ]),
  ).resolves.toBeUndefined();
  expect(request.mock.calls.every(([method]) => method === "session.get")).toBe(true);
});
