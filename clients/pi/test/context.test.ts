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
  const dir = await mkdtemp(join(tmpdir(), "llmparty-pi-context-"));
  tmpDirs.push(dir);
  return dir;
}

describe("loadTurnContext", () => {
  test("derives default context and log paths from LLMPARTY_RUNTIME_DIR", async () => {
    const runtimeDir = await tempWorkspace();
    expect(defaultCurrentTurnFile({ LLMPARTY_RUNTIME_DIR: runtimeDir, LLMPARTY_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "current-turn.json"),
    );
    expect(defaultHookLogFile({ LLMPARTY_RUNTIME_DIR: runtimeDir, LLMPARTY_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "pi-hook.log"),
    );
  });

  test("falls back to system temp directory when runtime directory is missing", () => {
    const fallbackDir = join(tmpdir(), "llmparty", "pi-runtime-fallback");

    expect(defaultCurrentTurnFile({ LLMPARTY_WORKSPACE: "/project" })).toBe(join(fallbackDir, "current-turn.json"));
    expect(defaultHookLogFile({ LLMPARTY_WORKSPACE: "/project" })).toBe(join(fallbackDir, "pi-hook.log"));
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
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({
      LLMPARTY_WORKSPACE: workspace,
      LLMPARTY_CURRENT_TURN_FILE: contextFile,
      LLMPARTY_INTERNAL_EVENT_URL: "http://from-env/internal/v1/events",
    });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_1",
      turnId: "turn_1",
      clientType: "pi",
      internalEventUrl: "http://from-env/internal/v1/events",
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
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({ LLMPARTY_WORKSPACE: workspace, LLMPARTY_CURRENT_TURN_FILE: contextFile });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context.internalEventUrl).toBe("http://from-file/internal/v1/events");
  });

  test("logs missing current-turn file and does not return fake context", async () => {
    const workspace = await tempWorkspace();
    const missingFile = join(workspace, "missing.json");
    const logFile = join(workspace, "hook.log");

    const result = await loadTurnContext({
      LLMPARTY_WORKSPACE: workspace,
      LLMPARTY_CURRENT_TURN_FILE: missingFile,
      LLMPARTY_PI_HOOK_LOG: logFile,
      LLMPARTY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
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
      LLMPARTY_WORKSPACE: workspace,
      LLMPARTY_CURRENT_TURN_FILE: contextFile,
      LLMPARTY_PI_HOOK_LOG: logFile,
      LLMPARTY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
    });

    expect(result.ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("invalid_current_turn_context");
    expect(log).toContain("turn_id is required");
    expect(log).toContain("client_type must be pi");
  });
});
