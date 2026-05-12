import { describe, expect, test, vi } from "vitest";
import { createLlmpartyPiExtension } from "../src/index.js";
import type { TurnContext } from "../src/context.js";
import type { InternalEvent } from "../src/events.js";

interface HandlerMap {
  [event: string]: (event: any, ctx: any) => Promise<void> | void;
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

function install(overrides: Partial<Parameters<typeof createLlmpartyPiExtension>[1]> = {}) {
  const { pi, handlers } = fakePi();
  const reported: InternalEvent[] = [];
  createLlmpartyPiExtension(pi as any, {
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

describe("llmparty pi extension lifecycle", () => {
  test("session_start startup reports one-time agent client ready from runtime env", async () => {
    const { handlers, reported } = install({
      env: {
        LLMPARTY_SESSION_ID: "sess_ready",
        LLMPARTY_RUNTIME_INSTANCE_ID: "rtinst_1",
        LLMPARTY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
    });

    await handlers.session_start({ reason: "startup" }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_ready",
      turn_id: null,
      source: "agent_client",
      client_type: "pi",
      payload: { runtime_instance_id: "rtinst_1" },
    });
  });

  test("session_start non-startup does not report ready", async () => {
    const { handlers, reported } = install({
      env: {
        LLMPARTY_SESSION_ID: "sess_ready",
        LLMPARTY_RUNTIME_INSTANCE_ID: "rtinst_1",
        LLMPARTY_INTERNAL_EVENT_URL: "http://localhost/internal/v1/events",
      },
    });

    await handlers.session_start({ reason: "reload" }, {});

    expect(reported).toEqual([]);
  });

  test("registers pi lifecycle handlers", () => {
    const { pi } = fakePi();
    createLlmpartyPiExtension(pi as any, {
      env: {},
      loadContext: vi.fn(),
      makeReporter: vi.fn(),
      logDiagnostic: vi.fn(),
    });

    expect(pi.on).toHaveBeenCalledWith("session_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("agent_start", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("message_update", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("message_end", expect.any(Function));
    expect(pi.on).toHaveBeenCalledWith("agent_end", expect.any(Function));
  });

  test("reads context on agent_start and reports output then completed on agent_end", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello " } }, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "world" } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
    expect(reported[0].payload).toEqual({ output: { summary: "hello world" } });
  });

  test("uses assistant message_end full text without TUI parsing", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_end({ message: { role: "assistant", content: [{ type: "text", text: "final answer" }] } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
    expect(reported[0].payload).toEqual({ output: { summary: "final answer" } });
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

    expect(reported.map((event) => event.type)).toEqual(["turn.output", "turn.completed"]);
  });

  test("reports turn.failed for explicit agent_end error", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.agent_end({ error: new Error("model failed"), messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.failed"]);
    expect(reported[0].payload).toEqual({ failure: { message: "model failed" } });
  });
});
