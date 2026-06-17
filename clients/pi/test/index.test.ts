import { access, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
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
    env: {},
    loadContext: vi.fn(async () => ({ ok: true as const, context, contextFile: "turn.json", logFile: "hook.log" })),
    makeReporter: vi.fn(() => ({ report: vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
      reported.push(event);
      return true;
    }) })),
    logDiagnostic: vi.fn(async () => undefined),
    ...overrides,
  });
  return { handlers, reported };
}

describe("pontia pi extension lifecycle", () => {
  test("session_start startup reports one-time agent client ready from runtime env", async () => {
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_1",
        getSessionFile: () => "/tmp/pi/session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => "/workspace",
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
        client_cwd: "/workspace",
      },
    });
  });

  test("session_start without managed runtime env binds through internal upsert and reports ready", async () => {
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      expect(url).toBe("http://localhost/internal/v1/runtime-bindings/upsert");
      expect(JSON.parse(String(init?.body))).toMatchObject({
        client_type: "pi",
        client_session_key: "pi_session_manual",
        client_session_file: "/tmp/pi/session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: "/workspace",
        launch_cwd: "/workspace",
        runtime_instance_id: expect.stringMatching(/^rtinst_/),
        tmux: {
          socket_path: "/tmp/tmux-1000/default",
          pane_id: "%42",
        },
      });
      return new Response(JSON.stringify({
        session: { session_id: "sess_bound" },
        runtime: {
          runtime_instance_id: "rtinst_bound",
          internal_event_url: "http://localhost/internal/v1/events",
          current_turn_file: "/tmp/pontia/current-turn.json",
        },
      }), { status: 200 });
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
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
        getCwd: () => "/workspace",
      },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_bound",
      payload: {
        runtime_instance_id: "rtinst_bound",
        client_session_key: "pi_session_manual",
      },
    });
  });

  test("session_start without pontia env reads pi settings pontia config and binds through discovered backend", async () => {
    const root = await tempDir();
    const settingsFile = join(root, ".pi", "agent", "settings.json");
    const pontiaConfig = join(root, ".config", "pontia-stable", "config.toml");
    await mkdir(join(root, ".pi", "agent"), { recursive: true });
    await mkdir(join(root, ".config", "pontia-stable"), { recursive: true });
    await writeFile(settingsFile, JSON.stringify({ pontia: { config: pontiaConfig } }));
    await writeFile(pontiaConfig, 'bind_addr = "127.0.0.1:18080"\nexternal_api_token = "stable-token"\n');

    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "http://127.0.0.1:18080/healthz") {
        return new Response("ok", { status: 200 });
      }
      expect(url).toBe("http://127.0.0.1:18080/internal/v1/runtime-bindings/upsert");
      expect(JSON.parse(String(init?.body))).toMatchObject({
        client_type: "pi",
        client_session_key: "pi_session_discovered",
        client_cwd: "/workspace",
      });
      return new Response(JSON.stringify({
        session: { session_id: "sess_discovered" },
        runtime: {
          runtime_instance_id: "rtinst_discovered",
          internal_event_url: "http://127.0.0.1:18080/internal/v1/events",
        },
      }), { status: 200 });
    });
    const { handlers, reported } = install({ env: { HOME: root }, fetch: fetchImpl as any });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_discovered", getCwd: () => "/workspace" },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(2);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_discovered",
      payload: { runtime_instance_id: "rtinst_discovered", client_session_key: "pi_session_discovered" },
    });
  });

  test("manual session binding omits tmux when pane identity is incomplete", async () => {
    const fetchImpl = vi.fn(async (_url: string, init?: RequestInit) => {
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
        PONTIA_INTERNAL_BINDING_UPSERT_URL: "http://localhost/internal/v1/runtime-bindings/upsert",
        TMUX: "/tmp/tmux-1000/default,2071,502",
      },
      fetch: fetchImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => "/workspace" },
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
  });

  test("agent_start consumes backend-delivered current-turn context after reporting started", async () => {
    const dir = await tempDir();
    const contextFile = join(dir, "current-turn.json");
    await writeFile(contextFile, JSON.stringify({
      session_id: "sess_consumed",
      input: "from web",
      inbox_message_id: "msg_consumed",
      client_type: "pi",
      runtime_instance_id: "rtinst_consumed",
      internal_event_url: "http://localhost/internal/v1/events",
    }));

    const { handlers, reported } = install({
      env: { PONTIA_CURRENT_TURN_FILE: contextFile, PONTIA_PI_HOOK_LOG: join(dir, "hook.log") },
      loadContext: loadTurnContext,
    });

    await handlers.agent_start({}, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_consumed",
      payload: { metadata: { inbox_message_id: "msg_consumed" } },
    });
    await expect(access(contextFile)).rejects.toThrow();
  });

  test("manual tui agent_start uses the bound session when current-turn context is absent", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({
      session: { session_id: "sess_bound" },
      runtime: {
        runtime_instance_id: "rtinst_bound",
        internal_event_url: "http://localhost/internal/v1/events",
        current_turn_file: "/tmp/pontia/current-turn.json",
      },
    }), { status: 200 }));
    const { handlers, reported } = install({
      env: { PONTIA_INTERNAL_BINDING_UPSERT_URL: "http://localhost/internal/v1/runtime-bindings/upsert" },
      fetch: fetchImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current-turn file is missing or unreadable: fallback/current-turn.json",
        contextFile: "fallback/current-turn.json",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => "/workspace" },
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

  test("session_start with partial managed runtime env does not attach or report ready", async () => {
    const fetchImpl = vi.fn(async () => new Response("unexpected", { status: 500 }));
    const { handlers, reported } = install({
      env: { PONTIA_SESSION_ID: "sess_partial" },
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
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
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
    expect(pi.on).toHaveBeenCalledWith("agent_end", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("session_shutdown", expect.any(Function));
    expect(pi.registerTool).not.toHaveBeenCalled();
  });

  test("session_shutdown quit reports session exited for managed runtime", async () => {
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => "/workspace" },
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

  test("session_shutdown replacement does not report session exited", async () => {
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
        PONTIA_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => "/workspace" },
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
        PONTIA_EXTERNAL_API_URL: "http://localhost/external/v1",
        PONTIA_EXTERNAL_API_TOKEN: "token",
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
        PONTIA_EXTERNAL_API_URL: "http://localhost/external/v1",
        PONTIA_EXTERNAL_API_TOKEN: "token",
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
    expect(reported[0].payload).toEqual({ runtime_instance_id: "rtinst_1", input: {} });
    expect(reported[1].payload).toEqual({ output: { summary: "hello world" } });
    expect(reported[3]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
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

  test("coalesces streaming message_update refresh hints and preserves terminal final update", async () => {
    vi.useFakeTimers();
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello " } }, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "world" } }, {});
    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);

    await vi.advanceTimersByTimeAsync(99);
    expect(reported.map((event) => event.type)).toEqual(["turn.started"]);

    await vi.advanceTimersByTimeAsync(1);
    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.message_updated"]);
    expect(reported[1]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "update" } });

    await handlers.agent_end({ messages: [] }, {});
    expect(reported.map((event) => event.type)).toEqual([
      "turn.started",
      "session.message_updated",
      "turn.output",
      "turn.completed",
      "session.message_updated",
    ]);
    expect(reported[4]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });

  test("generates a fresh pontia turn id for each real pi agent_start", async () => {
    const { handlers, reported } = install({
      loadContext: vi.fn(async () => ({
        ok: true as const,
        context: { ...context, turnId: undefined },
        contextFile: "turn.json",
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
      loadContext: vi.fn(async () => ({ ok: false as const, reason: "missing", contextFile: "missing.json", logFile: "hook.log" })),
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
        reason: "current-turn file is missing or unreadable: fallback/current-turn.json",
        contextFile: "fallback/current-turn.json",
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
    expect(reported[1].payload).toEqual({ failure: { message: "model failed" } });
    expect(reported[2]).toMatchObject({ source: "agent_client", turn_id: null, payload: { reason: "final" } });
  });
});
