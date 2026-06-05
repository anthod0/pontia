import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test } from "vitest";
import { defaultCurrentTurnFile, defaultHookLogFile, loadTurnContext } from "../src/context.js";

const tmpDirs: string[] = [];

afterEach(async () => {
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempWorkspace() {
  const dir = await mkdtemp(join(tmpdir(), "pilotfy-claude-context-"));
  tmpDirs.push(dir);
  return dir;
}

describe("loadTurnContext", () => {
  test("derives default context and log paths from PILOTFY_RUNTIME_DIR", async () => {
    const runtimeDir = await tempWorkspace();

    expect(defaultCurrentTurnFile({ PILOTFY_RUNTIME_DIR: runtimeDir })).toBe(join(runtimeDir, "current-turn.json"));
    expect(defaultHookLogFile({ PILOTFY_RUNTIME_DIR: runtimeDir })).toBe(join(runtimeDir, "claude-hook.log"));
  });

  test("silently skips missing fallback current-turn file when pilotfy env is absent", async () => {
    const workspace = await tempWorkspace();
    const logFile = join(workspace, "hook.log");

    const result = await loadTurnContext({ PILOTFY_CLAUDE_HOOK_LOG: logFile });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.silent).toBe(true);
    await expect(readFile(logFile, "utf8")).rejects.toMatchObject({ code: "ENOENT" });
  });

  test("loads claude_code context and lets environment event URL override file URL", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    await writeFile(
      contextFile,
      JSON.stringify({
        session_id: "sess_1",
        turn_id: "turn_1",
        input: "build it",
        client_type: "claude_code",
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({
      PILOTFY_CURRENT_TURN_FILE: contextFile,
      PILOTFY_INTERNAL_EVENT_URL: "http://from-env/internal/v1/events",
    });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_1",
      turnId: "turn_1",
      clientType: "claude_code",
      internalEventUrl: "http://from-env/internal/v1/events",
      input: "build it",
    });
  });

  test("rejects missing ids and non-claude_code client type", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    const logFile = join(workspace, "hook.log");
    await writeFile(contextFile, JSON.stringify({ session_id: "sess_2", client_type: "pi" }));

    const result = await loadTurnContext({
      PILOTFY_CURRENT_TURN_FILE: contextFile,
      PILOTFY_CLAUDE_HOOK_LOG: logFile,
      PILOTFY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
    });

    expect(result.ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("invalid_current_turn_context");
    expect(log).toContain("turn_id is required");
    expect(log).toContain("client_type must be claude_code");
  });
});
