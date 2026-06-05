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
  const dir = await mkdtemp(join(tmpdir(), "pilotfy-pi-context-"));
  tmpDirs.push(dir);
  return dir;
}

describe("loadTurnContext", () => {
  test("derives default context and log paths from PILOTFY_RUNTIME_DIR", async () => {
    const runtimeDir = await tempWorkspace();
    expect(defaultCurrentTurnFile({ PILOTFY_RUNTIME_DIR: runtimeDir, PILOTFY_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "current-turn.json"),
    );
    expect(defaultHookLogFile({ PILOTFY_RUNTIME_DIR: runtimeDir, PILOTFY_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "pi-hook.log"),
    );
  });

  test("falls back to system temp directory when runtime directory is missing", () => {
    const fallbackDir = join(tmpdir(), "pilotfy", "pi-runtime-fallback");

    expect(defaultCurrentTurnFile({ PILOTFY_WORKSPACE: "/project" })).toBe(join(fallbackDir, "current-turn.json"));
    expect(defaultHookLogFile({ PILOTFY_WORKSPACE: "/project" })).toBe(join(fallbackDir, "pi-hook.log"));
  });

  test("loads required turn ids and lets environment event URL override file URL", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    await writeFile(
      contextFile,
      JSON.stringify({
        session_id: "sess_1",
        turn_id: "turn_1",
        input: "build it",
        client_type: "pi",
        runtime_instance_id: "rtinst_file",
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({
      PILOTFY_WORKSPACE: workspace,
      PILOTFY_CURRENT_TURN_FILE: contextFile,
      PILOTFY_INTERNAL_EVENT_URL: "http://from-env/internal/v1/events",
      PILOTFY_RUNTIME_INSTANCE_ID: "rtinst_env",
    });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_1",
      turnId: "turn_1",
      clientType: "pi",
      internalEventUrl: "http://from-env/internal/v1/events",
      runtimeInstanceId: "rtinst_env",
      input: "build it",
    });
  });

  test("uses file event URL when environment URL is missing", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    await writeFile(
      contextFile,
      JSON.stringify({
        session_id: "sess_2",
        turn_id: "turn_2",
        client_type: "pi",
        runtime_instance_id: "rtinst_file_2",
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({ PILOTFY_WORKSPACE: workspace, PILOTFY_CURRENT_TURN_FILE: contextFile });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context.internalEventUrl).toBe("http://from-file/internal/v1/events");
    expect(result.context.runtimeInstanceId).toBe("rtinst_file_2");
  });

  test("silently skips missing fallback current-turn file when pilotfy env is absent", async () => {
    const workspace = await tempWorkspace();
    const logFile = join(workspace, "hook.log");

    const result = await loadTurnContext({ PILOTFY_PI_HOOK_LOG: logFile });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.silent).toBe(true);
    await expect(readFile(logFile, "utf8")).rejects.toMatchObject({ code: "ENOENT" });
  });

  test("logs missing current-turn file and does not return fake context", async () => {
    const workspace = await tempWorkspace();
    const missingFile = join(workspace, "missing.json");
    const logFile = join(workspace, "hook.log");

    const result = await loadTurnContext({
      PILOTFY_WORKSPACE: workspace,
      PILOTFY_CURRENT_TURN_FILE: missingFile,
      PILOTFY_PI_HOOK_LOG: logFile,
      PILOTFY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      PILOTFY_RUNTIME_INSTANCE_ID: "rtinst_missing_file",
    });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.reason).toContain("current-turn file");
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("missing_current_turn_file");
  });

  test("rejects missing ids and non-pi client type", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    const logFile = join(workspace, "hook.log");
    await writeFile(contextFile, JSON.stringify({ session_id: "sess_3", client_type: "generic" }));

    const result = await loadTurnContext({
      PILOTFY_WORKSPACE: workspace,
      PILOTFY_CURRENT_TURN_FILE: contextFile,
      PILOTFY_PI_HOOK_LOG: logFile,
      PILOTFY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
    });

    expect(result.ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("invalid_current_turn_context");
    expect(log).toContain("turn_id is required");
    expect(log).toContain("client_type must be pi");
    expect(log).toContain("runtime_instance_id or PILOTFY_RUNTIME_INSTANCE_ID is required");
  });
});
