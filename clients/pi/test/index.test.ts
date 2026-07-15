import { mkdir, mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import { loadTurnContext, type TurnContext } from "../src/context.js";
import type { InternalEvent } from "../src/events.js";

interface HandlerMap {
  [event: string]: (event: any, ctx: any) => Promise<any> | any;
}

function fakePi() {
  const handlers: HandlerMap = {};
  return {
    handlers,
    pi: {
      on: vi.fn((event: string, handler: HandlerMap[string]) => {
        handlers[event] = handler;
      }),
      registerTool: vi.fn(),
    },
  };
}

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
  internalEventUrl: "http://localhost/internal/v1/events",
};

const tmpDirs: string[] = [];
const defaultPontiaHome = mkdtempSync(join(tmpdir(), "pontia-pi-index-home-"));
writeFileSync(join(defaultPontiaHome, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');

afterEach(async () => {
  vi.useRealTimers();
  await Promise.all(tmpDirs.map((dir) => rm(dir, { recursive: true, force: true })));
  tmpDirs.length = 0;
});

async function tempDir() {
  const dir = await mkdtemp(join(tmpdir(), "pontia-pi-index-"));
  tmpDirs.push(dir);
  return dir;
}

function install(overrides: Partial<Parameters<typeof createPontiaPiExtension>[1]> = {}) {
  const { pi, handlers } = fakePi();
  const reported: InternalEvent[] = [];
  createPontiaPiExtension(pi as any, {
    loadContext: vi.fn(async () => ({ ok: true as const, context, logFile: "hook.log" })),
    makeReporter: vi.fn(() => ({ report: vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
      reported.push(event);
      return true;
    }) })),
    logDiagnostic: vi.fn(async () => undefined),
    ...overrides,
    env: { PONTIA_HOME: defaultPontiaHome, ...(overrides.env ?? {}) },
  });
  return { handlers, reported };
}

describe("pontia pi extension lifecycle", () => {
  test("session_start startup reports one-time agent client ready from runtime env", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/external/v1/workspaces");
      return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_1",
        getSessionFile: () => "/tmp/pi/session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_ready",
      turn_id: null,
      source: "agent_client",
      client_type: "pi",
      payload: {
        runtime_instance_id: "rtinst_1",
        client_session_key: "pi_session_1",
        client_session_file: "/tmp/pi/session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: workspace,
      },
    });
  });

  test("session_start new reports agent client ready from runtime env", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/external/v1/workspaces");
      return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_new",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_new",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "new" }, {
      sessionManager: {
        getSessionId: () => "pi_session_new",
        getSessionFile: () => "/tmp/pi/new-session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_new",
      payload: {
        runtime_instance_id: "rtinst_new",
        client_session_key: "pi_session_new",
        client_session_file: "/tmp/pi/new-session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: workspace,
      },
    });
  });

  test("manual session_start startup immediately reattaches when client session already has a pontia binding", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/agent-bindings?client_type=pi&client_session_key=pi_session_resumed") {
        return new Response(JSON.stringify({ data: { binding: { session_id: "sess_existing", client_type: "pi", client_session_key: "pi_session_resumed" } } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/runtime-bindings/upsert") {
        expect(JSON.parse(String(init?.body))).toMatchObject({
          client_type: "pi",
          client_session_key: "pi_session_resumed",
          client_session_file: "/tmp/pi/resumed.jsonl",
          runtime_instance_id: expect.stringMatching(/^rtinst_/),
          tmux: { socket_path: "/tmp/tmux-1000/default", pane_id: "%42" },
        });
        return new Response(JSON.stringify({
          session: { session_id: "sess_existing" },
          runtime: {
            runtime_instance_id: "rtinst_reattached",
            internal_event_url: "http://localhost/internal/v1/events",
          },
        }), { status: 200 });
      }
      return new Response(`unexpected ${url}`, { status: 500 });
    });
    const { handlers, reported } = install({
      env: {
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_resumed",
        getSessionFile: () => "/tmp/pi/resumed.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(3);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_existing",
      payload: {
        runtime_instance_id: "rtinst_reattached",
        client_session_key: "pi_session_resumed",
        client_session_file: "/tmp/pi/resumed.jsonl",
      },
    });
  });

  test("session_start without managed runtime env does not create a pontia session before first turn", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/agent-bindings?client_type=pi&client_session_key=pi_session_manual") {
        return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
      }
      return new Response("unexpected", { status: 500 });
    });
    const { handlers, reported } = install({
      env: {
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_manual",
        getSessionFile: () => "/tmp/pi/session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(2);
    expect(fetchImpl).not.toHaveBeenCalledWith("http://localhost/internal/v1/runtime-bindings/upsert", expect.anything());
    expect(reported).toEqual([]);
  });

  test("session_start skips manual binding when current workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 });
      }
      return new Response("unexpected", { status: 500 });
    });
    const logDiagnostic = vi.fn(async () => undefined);
    const { handlers, reported } = install({
      env: {
      },
      fetch: fetchImpl as any,
      logDiagnostic,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => workspace },
    });
    await handlers.before_agent_start({ prompt: "typed in tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(reported).toEqual([]);
    expect(logDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ code: "workspace_not_active" }));
  });

  test("session_start skips managed ready reporting when current workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      expect(url).toBe("http://localhost/external/v1/workspaces");
      return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "deleted" }] } }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });

    expect(reported).toEqual([]);
  });

  test("session_start without pontia env reads pontia home config but defers binding until first turn", async () => {
    const root = await tempDir();
    const workspace = await realpath(await tempDir());
    const pontiaConfig = join(root, ".pontia", "config.toml");
    await mkdir(join(root, ".pontia"), { recursive: true });
    await writeFile(pontiaConfig, 'bind_addr = "127.0.0.1:18080"\nexternal_api_token = "home-token"\n');

    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://127.0.0.1:18080/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url === "http://127.0.0.1:18080/internal/v1/agent-bindings?client_type=pi&client_session_key=pi_session_discovered") {
        return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
      }
      return new Response("unexpected", { status: 500 });
    });
    const { handlers, reported } = install({ env: { PONTIA_HOME: join(root, ".pontia") }, fetch: fetchImpl as any });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_discovered", getCwd: () => workspace },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(2);
    expect(reported).toEqual([]);
  });

  test("manual session binding on first turn omits tmux when pane identity is incomplete", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/agent-bindings?client_type=pi&client_session_key=pi_session_manual") {
        return new Response(JSON.stringify({ error: { code: "not_found" } }), { status: 404 });
      }
      expect(JSON.parse(String(init?.body))).not.toHaveProperty("tmux");
      return new Response(JSON.stringify({
        session: { session_id: "sess_bound" },
        runtime: {
          runtime_instance_id: "rtinst_bound",
          internal_event_url: "http://localhost/internal/v1/events",
        },
      }), { status: 200 });
    });
    const { handlers } = install({
      env: {
        TMUX: "/tmp/tmux-1000/default,2071,502",
      },
      fetch: fetchImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => workspace },
    });
    await handlers.before_agent_start({ prompt: "typed in tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => workspace },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(4);
  });

  test("agent_start consumes backend-delivered current-turn context after reporting started", async () => {
    const dir = await tempDir();
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
          data: {
            current_turn: {
              session_id: "sess_consumed",
              input: "from web",
              inbox_message_id: "msg_consumed",
              client_type: "pi",
              runtime_instance_id: "rtinst_consumed",
              internal_event_url: "http://localhost/internal/v1/events",
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    await writeFile(join(dir, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_consumed",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_consumed",
        PONTIA_HOME: dir,
      },
      fetch: fetchImpl as any,
      loadContext: (env) => loadTurnContext(env, { fetch: fetchImpl as any }),
    });

    await handlers.agent_start({}, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_consumed",
      payload: { metadata: { inbox_message_id: "msg_consumed" } },
    });
  });

  test("manual tui agent_start uses the bound session when current-turn context is absent", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      return new Response(JSON.stringify({
        session: { session_id: "sess_bound" },
        runtime: {
          runtime_instance_id: "rtinst_bound",
          internal_event_url: "http://localhost/internal/v1/events",
        },
      }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
      },
      fetch: fetchImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => workspace },
    });
    await handlers.before_agent_start({ prompt: "typed in tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "turn.started"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_bound",
      turn_id: expect.stringMatching(/^turn_/),
      payload: { runtime_instance_id: "rtinst_bound", input: { summary: "typed in tui" } },
    });
  });

  test("session_start fork binds a new pontia session with parent lineage", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      expect(url).toBe("http://localhost/internal/v1/runtime-bindings/upsert");
      const body = JSON.parse(String(init?.body));
      expect(body).toMatchObject({
        client_type: "pi",
        client_session_key: "pi_child",
        start_kind: "fork",
        parent_session_id: "sess_parent",
      });
      expect(body.runtime_instance_id).toMatch(/^rtinst_/);
      expect(body.runtime_instance_id).not.toBe("rtinst_parent");
      return new Response(JSON.stringify({
        session: { session_id: "sess_child" },
        runtime: {
          runtime_instance_id: "rtinst_child",
          internal_event_url: "http://localhost/internal/v1/events",
        },
      }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_parent",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_parent",
      },
      fetch: fetchImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_parent", getCwd: () => workspace },
    });
    await handlers.session_start({ reason: "fork" }, {
      sessionManager: { getSessionId: () => "pi_child", getCwd: () => workspace },
    });
    await handlers.before_agent_start({ prompt: "fork prompt", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "session.ready", "turn.started"]);
    expect(reported[1]).toMatchObject({ session_id: "sess_child", payload: { runtime_instance_id: "rtinst_child" } });
    expect(reported[2]).toMatchObject({ session_id: "sess_child", payload: { input: { summary: "fork prompt" } } });
  });

  test("session_start resume immediately reattaches when switched client session has a pontia binding", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async (url: string) => {
      if (url === "http://localhost/external/v1/workspaces") {
        return new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/agent-bindings?client_type=pi&client_session_key=pi_session_resume") {
        return new Response(JSON.stringify({ data: { binding: { session_id: "sess_resume" } } }), { status: 200 });
      }
      if (url === "http://localhost/internal/v1/runtime-bindings/upsert") {
        return new Response(JSON.stringify({
          session: { session_id: "sess_resume" },
          runtime: {
            runtime_instance_id: "rtinst_resume",
            internal_event_url: "http://localhost/internal/v1/events",
          },
        }), { status: 200 });
      }
      return new Response(`unexpected ${url}`, { status: 500 });
    });
    const { handlers, reported } = install({
      env: {
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "resume" }, {
      sessionManager: { getSessionId: () => "pi_session_resume", getCwd: () => workspace },
    });

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({ session_id: "sess_resume" });
  });

  test("session_start with partial managed runtime env does not attach or report ready", async () => {
    const home = await tempDir();
    const fetchImpl = vi.fn(async () => new Response("unexpected", { status: 500 }));
    const { handlers, reported } = install({
      env: { HOME: home, PONTIA_SESSION_ID: "sess_partial" },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_partial", getCwd: () => "/workspace" },
    });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("session_start non-startup does not report ready", async () => {
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
    });

    await handlers.session_start({ reason: "reload" }, {});

    expect(reported).toEqual([]);
  });

  test("registers pi lifecycle handlers without DAG agent tools", () => {
    const { pi } = fakePi();
    createPontiaPiExtension(pi as any, {
      env: { PONTIA_AGENT_KIND: "planner" },
      loadContext: vi.fn(),
      makeReporter: vi.fn(),
      logDiagnostic: vi.fn(),
    });

    expect(pi.on).toHaveBeenCalledWith("session_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("before_agent_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("agent_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("message_update", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("message_end", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("tool_execution_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("tool_execution_end", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("agent_end", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("session_shutdown", expect.any(Function));
    expect(pi.registerTool).not.toHaveBeenCalled();
  });

  test("session_shutdown quit reports session exited for managed runtime", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: "quit" }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "session.exited"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_exit",
      turn_id: null,
      source: "agent_client",
      client_type: "pi",
      payload: { reason: "quit", runtime_instance_id: "rtinst_1" },
    });
  });

  test.each(["new", "resume", "fork"])("session_shutdown %s reports session exited for managed runtime", async (shutdownReason) => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: shutdownReason }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "session.exited"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_exit",
      turn_id: null,
      source: "agent_client",
      client_type: "pi",
      payload: { reason: shutdownReason, runtime_instance_id: "rtinst_1" },
    });
  });

  test("session_shutdown reload does not report session exited", async () => {
    const workspace = await realpath(await tempDir());
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } }), { status: 200 }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: "reload" }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
  });

  test("appends profile system prompt from external API before agent starts", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      if (url.endsWith("/external/v1/sessions/sess_1")) {
        return new Response(JSON.stringify({ data: { session: { execution_profile_id: "planner", execution_profile_version: "1" } } }), { status: 200 });
      }
      if (url.endsWith("/external/v1/agent-profiles/planner/versions/1")) {
        return new Response(JSON.stringify({ data: { agent_profile: { system_prompt_template: "Planner instructions" } } }), { status: 200 });
      }
      return new Response("not found", { status: 404 });
    });
    const { handlers } = install({
      env: {
        PONTIA_SESSION_ID: "sess_1",
      },
      fetch: fetchImpl as any,
    });

    const result = await handlers.before_agent_start({ systemPrompt: "Base prompt" }, {});

    expect(result).toEqual({ systemPrompt: "Base prompt\n\nPlanner instructions" });
    expect(fetchImpl).toHaveBeenCalledWith("http://localhost/external/v1/sessions/sess_1", expect.objectContaining({ headers: { Authorization: "Bearer token" } }));
  });

  test("keeps original system prompt when profile has no system prompt", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      if (url.endsWith("/external/v1/sessions/sess_1")) {
        return new Response(JSON.stringify({ data: { session: { execution_profile_id: "default", execution_profile_version: "1" } } }), { status: 200 });
      }
      return new Response(JSON.stringify({ data: { agent_profile: { system_prompt_template: null } } }), { status: 200 });
    });
    const { handlers } = install({
      env: {
        PONTIA_SESSION_ID: "sess_1",
      },
      fetch: fetchImpl as any,
    });

    const result = await handlers.before_agent_start({ systemPrompt: "Base prompt" }, {});

    expect(result).toEqual({ systemPrompt: "Base prompt" });
  });

  test("reports context usage when a hook event exposes valid usage", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ context_usage: { used_tokens: 2, max_tokens: 8, usage_ratio: 0.25, confidence: "estimated" } }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.context_usage_updated"]);
    expect(reported[1]).toMatchObject({
      source: "agent_client",
      turn_id: "turn_1",
      payload: {
        context_usage: {
          used_tokens: 2,
          max_tokens: 8,
          usage_ratio: 0.25,
          confidence: "estimated",
        },
      },
    });
  });

  test("reports context usage from pi extension context", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello" } }, {
      model: { id: "gpt-5.5" },
      getContextUsage: () => ({ tokens: 6037, contextWindow: 128000, percent: 4.716 }),
    });

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.context_usage_updated"]);
    expect(reported[1]).toMatchObject({
      payload: {
        context_usage: {
          used_tokens: 6037,
          max_tokens: 128000,
          remaining_tokens: 121963,
          usage_ratio: 0.04716,
          confidence: "estimated",
        },
        model: "gpt-5.5",
      },
    });
  });

  test("does not report fake context usage when hook events do not expose usage", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello " } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).not.toContain("session.context_usage_updated");
  });

  test("reads context on agent_start and reports started, output, then completed", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello " } }, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "world" } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.output", "turn.completed", "session.message_updated"]);
    expect(reported[0].payload).toEqual({
      runtime_instance_id: "rtinst_1",
      input: {},
      timeline_anchor: { previous_leaf_id: null },
    });
    expect(reported[1].payload).toEqual({ output: { summary: "hello world" } });
    expect(reported[3]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });

  test("captures the previous and terminal Pi leaves at the lifecycle hook boundaries", async () => {
    const observations: string[] = [];
    let leafId: string | null = "entry_before_turn";
    let releaseStarted!: () => void;
    const startedReported = new Promise<void>((resolve) => {
      releaseStarted = resolve;
    });
    const report = vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
      observations.push(`report:${event.type}`);
      if (event.type === "turn.started") await startedReported;
      return true;
    });
    const { handlers } = install({ makeReporter: vi.fn(() => ({ report })) });
    const hookContext = {
      sessionManager: {
        getLeafId: () => {
          observations.push(`leaf:${leafId ?? "null"}`);
          return leafId;
        },
      },
    };

    const starting = handlers.agent_start({}, hookContext);
    await vi.waitFor(() => expect(report).toHaveBeenCalledTimes(1));
    expect(observations).toEqual(["leaf:entry_before_turn", "report:turn.started"]);
    expect(report.mock.calls[0]?.[1].payload).toMatchObject({
      timeline_anchor: { previous_leaf_id: "entry_before_turn" },
    });
    releaseStarted();
    await starting;

    leafId = "entry_after_turn";
    await handlers.agent_end({ messages: [] }, hookContext);

    const completed = report.mock.calls.find(([, event]) => event.type === "turn.completed")?.[1];
    expect(completed?.payload).toMatchObject({
      timeline_anchor: { terminal_leaf_id: "entry_after_turn" },
    });
    expect(observations.indexOf("leaf:entry_after_turn")).toBeLessThan(observations.indexOf("report:turn.completed"));
  });

  test("uses assistant message_end full text without TUI parsing", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_end({ message: { role: "assistant", content: [{ type: "text", text: "final answer" }] } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.message_updated", "turn.output", "turn.completed", "session.message_updated"]);
    expect(reported[1]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "append" } });
    expect(reported[2].payload).toEqual({ output: { summary: "final answer" } });
    expect(reported[4]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });

  test("reports transcript refresh hints for structured assistant stream boundaries but not text deltas", async () => {
    vi.useFakeTimers();
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { type: "thinking_start", contentIndex: 0, partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "thinking_delta", contentIndex: 0, delta: "reason", partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "thinking_end", contentIndex: 0, content: "reason", partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "text_start", contentIndex: 1, partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "text_delta", contentIndex: 1, delta: "hello", partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "text_end", contentIndex: 1, content: "hello", partial: {} } }, {});
    await vi.advanceTimersByTimeAsync(1000);

    expect(reported.map((event) => event.type)).toEqual([
      "turn.started",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
    ]);
    expect(reported.slice(1).map((event) => event.payload)).toEqual([
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
    ]);

    await handlers.agent_end({ messages: [] }, {});
    expect(reported.map((event) => event.type)).toEqual([
      "turn.started",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
      "turn.output",
      "turn.completed",
      "session.message_updated",
    ]);
    expect(reported[7]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });

  test("reports transcript refresh hints when tool calls start and finish successfully or with errors", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { type: "toolcall_start", contentIndex: 0, partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "toolcall_delta", contentIndex: 0, delta: "{}", partial: {} } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "toolcall_end", contentIndex: 0, toolCall: { type: "toolCall", id: "call_1", name: "read", arguments: { path: "README.md" } }, partial: {} } }, {});
    await handlers.tool_execution_start({ toolCallId: "call_1", toolName: "read", args: { path: "README.md" } }, {});
    await handlers.tool_execution_end({ toolCallId: "call_1", toolName: "read", result: {}, isError: false }, {});
    await handlers.tool_execution_end({ toolCallId: "call_2", toolName: "bash", result: {}, isError: true }, {});

    expect(reported.map((event) => event.type)).toEqual([
      "turn.started",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
      "session.message_updated",
    ]);
    expect(reported.slice(1).map((event) => event.payload)).toEqual([
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
    ]);
  });

  test("generates a fresh pontia turn id for each real pi agent_start", async () => {
    const { handlers, reported } = install({
      loadContext: vi.fn(async () => ({
        ok: true as const,
        context: { ...context, turnId: undefined },
        logFile: "hook.log",
      })),
    });

    await handlers.before_agent_start({ prompt: "first from dashboard", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});
    await handlers.agent_end({ messages: [{ role: "assistant", content: "first answer" }] }, {});

    await handlers.before_agent_start({ prompt: "second from tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});
    await handlers.agent_end({ messages: [{ role: "assistant", content: "second answer" }] }, {});

    const started = reported.filter((event) => event.type === "turn.started");
    expect(started).toHaveLength(2);
    expect(started[0].turn_id).toMatch(/^turn_/);
    expect(started[1].turn_id).toMatch(/^turn_/);
    expect(started[1].turn_id).not.toBe(started[0].turn_id);
    expect(reported.filter((event) => event.type === "turn.output").map((event) => event.turn_id)).toEqual([
      started[0].turn_id,
      started[1].turn_id,
    ]);
    expect(reported.map((event) => event.type)).not.toContain("turn.created");
  });

  test("does not report completion when context is missing", async () => {
    const { handlers, reported } = install({
      loadContext: vi.fn(async () => ({ ok: false as const, reason: "missing", logFile: "hook.log" })),
    });

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello" } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported).toEqual([]);
  });

  test("does not show a UI warning when missing context is a silent manual session skip", async () => {
    const notify = vi.fn();
    const { handlers, reported } = install({
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.agent_start({}, { hasUI: true, ui: { notify } });

    expect(notify).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("ignores duplicate agent_end for the same active turn", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello" } }, {});
    await handlers.agent_end({ messages: [] }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.output", "turn.completed", "session.message_updated"]);
  });

  test("reports turn.failed for explicit agent_end error", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.agent_end({ error: new Error("model failed"), messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.failed", "session.message_updated"]);
    expect(reported[1].payload).toEqual({
      runtime_instance_id: "rtinst_1",
      failure: { message: "model failed" },
      timeline_anchor: { terminal_leaf_id: null },
    });
    expect(reported[2]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });
});
