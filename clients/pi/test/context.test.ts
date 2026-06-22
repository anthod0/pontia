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
  const dir = await mkdtemp(join(tmpdir(), "pontia-pi-context-"));
  tmpDirs.push(dir);
  return dir;
}

describe("loadTurnContext", () => {
  test("derives default context and log paths from PONTIA_RUNTIME_DIR", async () => {
    const runtimeDir = await tempWorkspace();
    expect(defaultCurrentTurnFile({ PONTIA_RUNTIME_DIR: runtimeDir, PONTIA_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "current-turn.json"),
    );
    expect(defaultHookLogFile({ PONTIA_RUNTIME_DIR: runtimeDir, PONTIA_WORKSPACE: "/project" })).toBe(
      join(runtimeDir, "pi-hook.log"),
    );
  });

  test("falls back to system temp directory when runtime directory is missing", () => {
    const fallbackDir = join(tmpdir(), "pontia", "pi-runtime-fallback");

    expect(defaultCurrentTurnFile({ PONTIA_WORKSPACE: "/project" })).toBe(join(fallbackDir, "current-turn.json"));
    expect(defaultHookLogFile({ PONTIA_WORKSPACE: "/project" })).toBe(join(fallbackDir, "pi-hook.log"));
  });

  test("claims current turn from internal API when session context is available", async () => {
    const workspace = await tempWorkspace();
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetchImpl = (async (url: string | URL | Request, init?: RequestInit) => {
      calls.push({ url: String(url), init });
      return new Response(
        JSON.stringify({
          data: {
            current_turn: {
              session_id: "sess_api",
              input: "from web ui",
              inbox_message_id: "msg_api",
              client_type: "pi",
              runtime_instance_id: "rtinst_api",
              internal_event_url: "http://from-api/internal/v1/events",
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }) as typeof fetch;

    const result = await loadTurnContext(
      {
        PONTIA_WORKSPACE: workspace,
        PONTIA_SESSION_ID: "sess_api",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_api",
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
      { fetch: fetchImpl },
    );

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("http://localhost/internal/v1/sessions/sess_api/current-turn/claim");
    expect(calls[0]?.init?.method).toBe("POST");
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_api",
      turnId: undefined,
      clientType: "pi",
      internalEventUrl: "http://localhost/internal/v1/events",
      runtimeInstanceId: "rtinst_api",
      input: "from web ui",
      inboxMessageId: "msg_api",
    });
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
      PONTIA_WORKSPACE: workspace,
      PONTIA_CURRENT_TURN_FILE: contextFile,
      PONTIA_INTERNAL_EVENT_URL: "http://from-env/internal/v1/events",
      PONTIA_RUNTIME_INSTANCE_ID: "rtinst_env",
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

    const result = await loadTurnContext({ PONTIA_WORKSPACE: workspace, PONTIA_CURRENT_TURN_FILE: contextFile });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context.internalEventUrl).toBe("http://from-file/internal/v1/events");
    expect(result.context.runtimeInstanceId).toBe("rtinst_file_2");
  });

  test("silently skips missing fallback current-turn file when pontia env is absent", async () => {
    const workspace = await tempWorkspace();
    const logFile = join(workspace, "hook.log");

    const result = await loadTurnContext({ PONTIA_PI_HOOK_LOG: logFile });

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
      PONTIA_WORKSPACE: workspace,
      PONTIA_CURRENT_TURN_FILE: missingFile,
      PONTIA_PI_HOOK_LOG: logFile,
      PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      PONTIA_RUNTIME_INSTANCE_ID: "rtinst_missing_file",
    });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.reason).toContain("current-turn file");
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("missing_current_turn_file");
  });

  test("accepts pending input context without turn_id because pi plugin owns turn identity", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    await writeFile(
      contextFile,
      JSON.stringify({
        session_id: "sess_3",
        input: "typed in web ui",
        inbox_message_id: "msg_3",
        client_type: "pi",
        runtime_instance_id: "rtinst_file_3",
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({ PONTIA_WORKSPACE: workspace, PONTIA_CURRENT_TURN_FILE: contextFile });

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_3",
      turnId: undefined,
      input: "typed in web ui",
      inboxMessageId: "msg_3",
      runtimeInstanceId: "rtinst_file_3",
    });
  });

  test("rejects missing session/runtime ids and non-pi client type", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    const logFile = join(workspace, "hook.log");
    await writeFile(contextFile, JSON.stringify({ session_id: "sess_4", client_type: "generic" }));

    const result = await loadTurnContext({
      PONTIA_WORKSPACE: workspace,
      PONTIA_CURRENT_TURN_FILE: contextFile,
      PONTIA_PI_HOOK_LOG: logFile,
      PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
    });

    expect(result.ok).toBe(false);
    const log = await readFile(logFile, "utf8");
    expect(log).toContain("invalid_current_turn_context");
    expect(log).toContain("client_type must be pi");
    expect(log).toContain("runtime_instance_id or PONTIA_RUNTIME_INSTANCE_ID is required");
    expect(log).not.toContain("turn_id is required");
  });
});
