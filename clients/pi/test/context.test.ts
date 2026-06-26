import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test } from "vitest";
import { defaultHookLogFile, loadTurnContext } from "../src/context.js";

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

async function pontiaHomeWithConfig(bindAddr = "localhost:80") {
  const dir = await tempWorkspace();
  await writeFile(join(dir, "config.toml"), `bind_addr = "${bindAddr}"\nexternal_api_token = "token"\n`);
  return dir;
}

describe("loadTurnContext", () => {
  test("falls back to pontia home state directory for hook log when explicit log directory is missing", async () => {
    const home = await tempWorkspace();
    expect(defaultHookLogFile({ HOME: home, PONTIA_WORKSPACE: "/project" })).toBe(
      join(home, ".pontia", "state", "pi-hook.log"),
    );
  });

  test("uses PONTIA_HOME for the default hook log path", async () => {
    const pontiaHome = await tempWorkspace();
    expect(defaultHookLogFile({ PONTIA_HOME: pontiaHome, PONTIA_WORKSPACE: "/project" })).toBe(
      join(pontiaHome, "state", "pi-hook.log"),
    );
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

    const pontiaHome = await pontiaHomeWithConfig();
    const result = await loadTurnContext(
      {
        PONTIA_WORKSPACE: workspace,
        PONTIA_HOME: pontiaHome,
        PONTIA_SESSION_ID: "sess_api",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_api",
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
      internalEventUrl: "http://from-api/internal/v1/events",
      runtimeInstanceId: "rtinst_api",
      input: "from web ui",
      inboxMessageId: "msg_api",
    });
  });

  test("does not read current-turn files when API claim is unavailable", async () => {
    const workspace = await tempWorkspace();
    const contextFile = join(workspace, "turn.json");
    const logFile = join(workspace, "state", "pi-hook.log");
    await writeFile(
      contextFile,
      JSON.stringify({
        session_id: "sess_file",
        input: "stale file input",
        client_type: "pi",
        runtime_instance_id: "rtinst_file",
        internal_event_url: "http://from-file/internal/v1/events",
      }),
    );

    const result = await loadTurnContext({
      PONTIA_WORKSPACE: workspace,
      PONTIA_HOME: workspace,
    });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.reason).toBe("current turn claim unavailable");
    expect(result.silent).toBe(true);
    await expect(readFile(logFile, "utf8")).rejects.toMatchObject({ code: "ENOENT" });
  });

  test("silently skips when pontia API claim env is absent", async () => {
    const workspace = await tempWorkspace();
    const logFile = join(workspace, "state", "pi-hook.log");

    const result = await loadTurnContext({ PONTIA_HOME: workspace });

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected missing context");
    expect(result.silent).toBe(true);
    await expect(readFile(logFile, "utf8")).rejects.toMatchObject({ code: "ENOENT" });
  });

  test("accepts pending input context without turn_id because pi plugin owns turn identity", async () => {
    const fetchImpl = (async () =>
      new Response(
        JSON.stringify({
          data: {
            current_turn: {
              session_id: "sess_3",
              input: "typed in web ui",
              inbox_message_id: "msg_3",
              client_type: "pi",
              runtime_instance_id: "rtinst_file_3",
              internal_event_url: "http://from-api/internal/v1/events",
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )) as typeof fetch;

    const pontiaHome = await pontiaHomeWithConfig();
    const result = await loadTurnContext(
      {
        PONTIA_HOME: pontiaHome,
        PONTIA_SESSION_ID: "sess_3",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_file_3",
      },
      { fetch: fetchImpl },
    );

    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected context");
    expect(result.context).toMatchObject({
      sessionId: "sess_3",
      turnId: undefined,
      input: "typed in web ui",
      inboxMessageId: "msg_3",
      runtimeInstanceId: "rtinst_file_3",
      internalEventUrl: "http://from-api/internal/v1/events",
    });
  });

  test("rejects missing session/runtime ids and non-pi client type from API claim", async () => {
    const fetchImpl = (async () =>
      new Response(
        JSON.stringify({ data: { current_turn: { session_id: "sess_4", client_type: "generic" } } }),
        { status: 200, headers: { "content-type": "application/json" } },
      )) as typeof fetch;

    const pontiaHome = await pontiaHomeWithConfig();
    const result = await loadTurnContext(
      {
        PONTIA_HOME: pontiaHome,
        PONTIA_SESSION_ID: "sess_4",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_4",
      },
      { fetch: fetchImpl },
    );

    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected invalid context");
    expect(result.reason).toContain("client_type must be pi");
    expect(result.reason).not.toContain("turn_id is required");
  });
});
