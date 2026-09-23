import { mkdir, realpath } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { type PiConnection } from "../src/control-socket.js";
import { createPontiaPiExtension } from "../src/index.js";
import { loadTurnContext, type TurnContext } from "../src/context.js";
import type { InternalEvent } from "../src/events.js";
import type { LiveOutputPublisherLike } from "../src/live-output.js";
import { tempDir as isolatedTempDir } from "./temp-dir.js";

interface HandlerMap {
  [event: string]: (event: any, ctx: any) => Promise<any> | any;
}

function fakePi() {
  const handlers: HandlerMap = {};
  const commands: Record<string, { handler: (args: string, ctx: any) => Promise<void> | void }> = {};
  const sendUserMessage = vi.fn();
  return {
    handlers,
    commands,
    sendUserMessage,
    pi: {
      on: vi.fn((event: string, handler: HandlerMap[string]) => {
        handlers[event] = event === "session_start"
          ? (sessionEvent, ctx = {}) => handler(sessionEvent, {
              mode: "tui",
              ...ctx,
              sessionManager: {
                getSessionFile: () => "/tmp/pi/default-session.jsonl",
                ...(ctx.sessionManager ?? {}),
              },
            })
          : handler;
      }),
      registerTool: vi.fn(),
      registerCommand: vi.fn((name: string, command: { handler: (args: string, ctx: any) => Promise<void> | void }) => {
        commands[name] = command;
      }),
      sendUserMessage,
    },
  };
}

function persistentTuiContext<T extends Record<string, unknown>>(ctx: T): T & {
  mode: "tui";
  sessionManager: { getSessionFile(): string };
} {
  return {
    mode: "tui",
    sessionManager: { getSessionFile: () => "/tmp/pi/default-session.jsonl" },
    ...ctx,
  };
}

const context: TurnContext = {
  sessionId: "sess_1",
  turnId: "turn_1",
  runtimeInstanceId: "rtinst_1",
  clientType: "pi",
};

let defaultPontiaHome: string;

async function tempDir() {
  return isolatedTempDir("pontia-pi-index-");
}

beforeEach(async () => {
  defaultPontiaHome = await isolatedTempDir("pontia-pi-index-home-");
});

afterEach(() => {
  vi.useRealTimers();
});

function install(overrides: Partial<Parameters<typeof createPontiaPiExtension>[1]> & { request?: PiConnection["request"] } = {}) {
  const { pi, handlers, commands, sendUserMessage } = fakePi();
  const reported: InternalEvent[] = [];
  let turnSequence = 0;
  const env: Record<string, string | undefined> = {
    PONTIA_HOME: defaultPontiaHome,
    TMUX: "/tmp/tmux-1000/default,2071,502",
    TMUX_PANE: "%42",
    ...(overrides.env ?? {}),
  };
  const managedRuntime = env.PONTIA_SESSION_ID && env.PONTIA_RUNTIME_INSTANCE_ID
    ? { sessionId: env.PONTIA_SESSION_ID, runtimeInstanceId: env.PONTIA_RUNTIME_INSTANCE_ID }
    : undefined;
  delete env.PONTIA_SESSION_ID;
  delete env.PONTIA_RUNTIME_INSTANCE_ID;
  const suppliedRequest = overrides.request;
  let paneManaged = managedRuntime !== undefined || suppliedRequest === undefined;
  const request: PiConnection["request"] = async (method, params) => {
    const record = params as Record<string, any>;
    if (managedRuntime && method === "session.context") return { session_context: null };
    if (managedRuntime && method === "runtime.register" && !record.binding.start_kind) {
      paneManaged = true;
      return {
        session: { session_id: managedRuntime.sessionId },
        runtime: { runtime_instance_id: managedRuntime.runtimeInstanceId },
      };
    }
    if (!suppliedRequest) throw new Error(`Unexpected RPC: ${method}`);
    const result = await suppliedRequest(method, method === "runtime.register" ? record.binding : params);
    if (method === "runtime.register") paneManaged = true;
    return result;
  };
  const connection: PiConnection = { request, registered() {}, async close() {} };
  const suppliedConnect = overrides.connectPi;
  createPontiaPiExtension(pi as any, {

    loadContext: vi.fn(async () => ({ ok: true as const, context, logFile: "hook.log" })),
    makeReporter: vi.fn(() => ({ report: vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
      reported.push(event);
      turnSequence += event.type === "turn.started" ? 1 : 0;
      return { accepted: true, eventId: `evt_server_${reported.length}`, turnId: event.type === "turn.started" ? `turn_server_${turnSequence}` : event.turn_id };
    }) })),
    logDiagnostic: vi.fn(async () => undefined),
    loadManagedRuntime: vi.fn(async () => managedRuntime),
    isManagedPane: vi.fn(async () => paneManaged),
    makeLiveOutputPublisher: vi.fn((): LiveOutputPublisherLike => ({
      appendText: vi.fn(),
      appendToolCall: vi.fn(),
      close: vi.fn(async () => undefined),
    })),
    ...overrides,
    connectPi: async (...args) => suppliedConnect ? { ...await suppliedConnect(...args), request } : connection,
    env,
  });
  return { handlers, commands, sendUserMessage, reported, env };
}

describe("pontia pi extension lifecycle", () => {
  test("socket input retains its Inbox identity across async hooks and never labels manual input", async () => {
    let submit: ((input: { input: string; inboxMessageId?: string }) => void) | undefined;
    let idle = true;
    const workspace = await realpath(await tempDir());
    const loadContext = vi.fn(async () => ({ ok: false as const, silent: true, reason: "no pending input", logFile: "hook.log" }));
    const { handlers, reported, sendUserMessage } = install({
      env: { PONTIA_SESSION_ID: "sess_direct", PONTIA_RUNTIME_INSTANCE_ID: "rtinst_direct" },
      request: vi.fn(async () => ({ workspaces: [{ canonical_path: workspace, state: "active" }] })) as any,
      loadContext,
      connectPi: async (_home, _onError, onSubmit) => {
        submit = onSubmit;
        return { request: async () => null, registered() {}, close: async () => {} };
      },
    });
    const ctx = { isIdle: () => idle, sessionManager: {
      getSessionId: () => "pi_direct", getCwd: () => workspace,
    } };
    await handlers.session_start({ reason: "startup" }, ctx);
    let started: Promise<void> | undefined;
    let retry: Promise<void> | undefined;
    let releaseRetry!: () => void;
    const retryGate = new Promise<void>((resolve) => { releaseRetry = resolve; });
    sendUserMessage.mockImplementation((input: string) => {
      started = (async () => {
        await new Promise((resolve) => setTimeout(resolve, 0));
        await handlers.before_agent_start({ prompt: input }, ctx);
        await handlers.agent_start({}, ctx);
      })();
      if (input === "from dashboard") {
        retry = (async () => {
          await retryGate;
          await handlers.before_agent_start({ prompt: "retry continuation" }, ctx);
          await handlers.agent_start({}, ctx);
        })();
      }
    });
    submit!({ input: "from dashboard", inboxMessageId: "msg_direct" });
    expect(reported.filter((event) => event.type === "turn.started")).toHaveLength(0);
    await started;
    const first = reported.find((event) => event.type === "turn.started")!;
    expect(first.data).toMatchObject({ input_summary: "from dashboard", inbox_message_id: "msg_direct" });
    await handlers.agent_end({ messages: [] }, ctx);
    releaseRetry();
    await retry;
    const continuation = reported.filter((event) => event.type === "turn.started")[1];
    expect(continuation.data).toMatchObject({ input_summary: "retry continuation" });
    expect(continuation.data).not.toHaveProperty("inbox_message_id");

    idle = false;
    expect(() => submit!({ input: "too early" })).toThrow("Pi is busy");
    await handlers.agent_end({ messages: [] }, ctx);
    submit!({ input: "next from inbox", inboxMessageId: "msg_next" });
    expect(sendUserMessage).toHaveBeenCalledTimes(1);
    idle = true;
    await handlers.agent_settled({}, ctx);
    await started;
    expect(sendUserMessage).toHaveBeenCalledTimes(2);
    const second = reported.filter((event) => event.type === "turn.started")[2];
    expect(second.data).toMatchObject({ inbox_message_id: "msg_next" });
    await handlers.agent_end({ messages: [] }, ctx);

    await handlers.before_agent_start({ prompt: "typed manually" }, ctx);
    await handlers.agent_start({}, ctx);
    const manual = reported.filter((event) => event.type === "turn.started")[3];
    expect(manual.data).toMatchObject({ input_summary: "typed manually" });
    expect(manual.data).not.toHaveProperty("inbox_message_id");
    await handlers.session_shutdown({ reason: "quit" }, ctx);
    expect(() => submit!({ input: "stale" })).toThrow("no longer current");
  });

  test("pontia-edit resolves, navigates once without summarization, clears restored text, then submits replacement", async () => {
    const calls: string[] = [];
    const requestImpl = vi.fn(async (method: string, params?: any) => {
      if (method !== "branch.resolve") {
        return { workspaces: [{ canonical_path: defaultPontiaHome, state: "active" }] };
      }
      calls.push("resolve");
      expect(params).toEqual({
        inbox_message_id: "msg_replay",
        session_id: "sess_replay",
        runtime_instance_id: "rtinst_replay",
        client_type: "pi",
      });
      return {
        branch_replay: {
          inbox_message_id: "msg_replay",
          session_id: "sess_replay",
          runtime_instance_id: "rtinst_replay",
          client_type: "pi",
          replacement_input: "replacement prompt",
          target_entry_id: "native-user",
        },
      };
    });
    let replay: ((message: string) => void) | undefined;
    const { handlers, reported, commands, sendUserMessage } = install({
      connectPi: async (_home, _error, _submit, _models, onReplay) => {
        replay = onReplay;
        return { request: vi.fn(), registered() {}, async close() {} };
      },
      env: {
        PONTIA_SESSION_ID: "sess_replay",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_replay",
      },
      request: requestImpl as any,
      loadContext: vi.fn(async () => ({ ok: false as const, silent: true, reason: "no pending input", logFile: "hook.log" })),
    });
    const ctx = persistentTuiContext({ isIdle: () => true, sessionManager: {
      getSessionFile: () => "/tmp/pi/default-session.jsonl",
      getSessionId: () => "pi_replay", getCwd: () => defaultPontiaHome,
    } });
    await handlers.session_start({ reason: "startup" }, ctx);
    let started: Promise<void> | undefined;
    let command: Promise<void> | void = undefined;
    sendUserMessage.mockImplementation((input: string, options?: { expandPromptTemplates?: boolean }) => {
      if (options?.expandPromptTemplates) {
        command = commands["pontia-edit"].handler(input.slice("/pontia-edit ".length), persistentTuiContext(commandContext));
        return;
      }
      calls.push("send");
      started = (async () => {
        await Promise.resolve();
        await handlers.before_agent_start({ prompt: input }, ctx);
        await handlers.agent_start({}, ctx);
      })();
    });
    const commandContext = {
      waitForIdle: vi.fn(async () => calls.push("idle")),
      navigateTree: vi.fn(async () => {
        calls.push("navigate");
        return { cancelled: false };
      }),
      ui: { setEditorText: vi.fn(() => calls.push("clear")) },
    };

    replay!("msg_replay");
    await command;

    await started;
    expect(reported.find((event) => event.type === "turn.started")?.data)
      .toMatchObject({ input_summary: "replacement prompt", inbox_message_id: "msg_replay" });
    expect(sendUserMessage).toHaveBeenCalledWith("/pontia-edit msg_replay", { expandPromptTemplates: true });
    expect(commandContext.waitForIdle).toHaveBeenCalledOnce();
    expect(commandContext.navigateTree).toHaveBeenCalledOnce();
    expect(commandContext.navigateTree).toHaveBeenCalledWith("native-user", { summarize: false });
    expect(commandContext.ui.setEditorText).toHaveBeenCalledWith("");
    expect(sendUserMessage).toHaveBeenCalledWith("replacement prompt");
    expect(calls).toEqual(["resolve", "idle", "navigate", "clear", "send"]);
    await handlers.session_shutdown({ reason: "quit" }, ctx);
    expect(() => replay!("msg_stale")).toThrow("no longer current");
  });

  test.each([
    ["cancelled navigation", { cancelled: true }, undefined, "branch_replay_navigation_cancelled"],
    ["navigation failure", undefined, new Error("navigation failed"), "branch_replay_navigation_failed"],
  ])("pontia-edit diagnoses %s and never submits replacement", async (_name, navigationResult, navigationError, code) => {
    const logDiagnostic = vi.fn(async () => undefined);
    const { commands, sendUserMessage } = install({
      env: {
        PONTIA_SESSION_ID: "sess_replay",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_replay",
      },
      request: vi.fn(async () => ({
        branch_replay: {
          inbox_message_id: "msg_replay",
          session_id: "sess_replay",
          runtime_instance_id: "rtinst_replay",
          client_type: "pi",
          replacement_input: "replacement prompt",
          target_entry_id: "native-user",
        },
      })) as any,
      logDiagnostic,
    });
    const navigateTree = navigationError
      ? vi.fn(async () => { throw navigationError; })
      : vi.fn(async () => navigationResult);

    await commands["pontia-edit"].handler("msg_replay", persistentTuiContext({
      waitForIdle: vi.fn(async () => undefined),
      navigateTree,
      ui: { setEditorText: vi.fn() },
    }));

    expect(sendUserMessage).not.toHaveBeenCalled();
    expect(logDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ code }));
  });

  test("pontia-edit diagnoses resolution and submission failures without retrying navigation", async () => {
    const resolveDiagnostic = vi.fn(async () => undefined);
    const failedResolve = install({
      env: {
        PONTIA_SESSION_ID: "sess_replay",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_replay",
      },
      request: vi.fn(async () => Promise.reject(new Error("stale runtime"))) as any,
      logDiagnostic: resolveDiagnostic,
    });
    const resolveNavigation = vi.fn();
    await failedResolve.commands["pontia-edit"].handler("msg_replay", persistentTuiContext({
      waitForIdle: vi.fn(),
      navigateTree: resolveNavigation,
      ui: { setEditorText: vi.fn() },
    }));
    expect(resolveNavigation).not.toHaveBeenCalled();
    expect(failedResolve.sendUserMessage).not.toHaveBeenCalled();
    expect(resolveDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({
      code: "branch_replay_resolve_failed",
    }));

    const submitDiagnostic = vi.fn(async () => undefined);
    const failedSubmit = install({
      env: {
        PONTIA_SESSION_ID: "sess_replay",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_replay",
      },
      request: vi.fn(async () => ({
        branch_replay: {
          inbox_message_id: "msg_replay",
          session_id: "sess_replay",
          runtime_instance_id: "rtinst_replay",
          client_type: "pi",
          replacement_input: "replacement prompt",
          target_entry_id: "native-user",
        },
      })) as any,
      logDiagnostic: submitDiagnostic,
    });
    failedSubmit.sendUserMessage.mockImplementation(() => {
      throw new Error("send failed");
    });
    const submitNavigation = vi.fn(async () => ({ cancelled: false }));
    await failedSubmit.commands["pontia-edit"].handler("msg_replay", persistentTuiContext({
      waitForIdle: vi.fn(async () => undefined),
      navigateTree: submitNavigation,
      ui: { setEditorText: vi.fn() },
    }));
    expect(submitNavigation).toHaveBeenCalledOnce();
    expect(submitDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({
      code: "branch_replay_submission_failed",
    }));
  });

  test("session_start startup reports one-time agent client ready from runtime env", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      expect(method).toBe("workspaces.list");
      return { workspaces: [{ canonical_path: workspace, state: "active" }] };
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
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
      data: {
        runtime_instance_id: "rtinst_1",
        client_session_key: "pi_session_1",
        client_session_file: "/tmp/pi/session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: workspace,
      },
    });
  });

  test.each([
    ["print mode", "print", "/tmp/pi/session.jsonl"],
    ["json mode", "json", "/tmp/pi/session.jsonl"],
    ["rpc mode", "rpc", "/tmp/pi/session.jsonl"],
    ["--no-session", "tui", undefined],
  ])("session_start ignores %s", async (_case, mode, sessionFile) => {
    const requestImpl = vi.fn(async () => ({ workspaces: [{ canonical_path: "/workspace", state: "active" }] }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ignored",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_ignored",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      mode,
      sessionManager: {
        getSessionId: () => "pi_session_ignored",
        getSessionFile: () => sessionFile,
        getSessionDir: () => "/tmp/pi",
        getCwd: () => "/workspace",
      },
    });

    expect(requestImpl).not.toHaveBeenCalled();
    expect(reported).toEqual([]);
  });

  test("session_start new reports agent client ready from runtime env", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      expect(method).toBe("workspaces.list");
      return { workspaces: [{ canonical_path: workspace, state: "active" }] };
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_new",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_new",
      },
      request: requestImpl as any,
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
      data: {
        runtime_instance_id: "rtinst_new",
        client_session_key: "pi_session_new",
        client_session_file: "/tmp/pi/new-session.jsonl",
        client_session_dir: "/tmp/pi",
        client_cwd: workspace,
      },
    });
  });

  test("stale tmux runtime markers do not identify a new native pi session", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string, params?: any) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: null };
      }
      if (method === "runtime.register") {
        const body = params;
        expect(body).toMatchObject({
          client_session_key: "pi_session_fresh",
          tmux: { socket_path: "/tmp/tmux-1000/default", pane_id: "%42" },
        });
        expect(body).not.toHaveProperty("session_id");
        expect(body).not.toHaveProperty("runtime_instance_id");
        return {
          session: { session_id: "sess_fresh" },
          runtime: {
            runtime_instance_id: "rtinst_fresh",
          },
        };
      }
      return Promise.reject(new Error(`unexpected ${method}`));
    });
    const { handlers, reported } = install({
      request: requestImpl as any,
      loadManagedRuntime: vi.fn(async () => ({ sessionId: "sess_stale", runtimeInstanceId: "rtinst_stale" })),
      isManagedPane: vi.fn(async () => true),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_fresh",
        getSessionFile: () => "/tmp/pi/fresh.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(reported[0]).toMatchObject({
      session_id: "sess_fresh",
      data: { runtime_instance_id: "rtinst_fresh", client_session_key: "pi_session_fresh" },
    });
  });

  test.each(["idle", "busy", "interrupted"])(
    "a %s pontia session for the native key suppresses duplicate TUI binding",
    async (sessionState) => {
      const workspace = await realpath(await tempDir());
      const requestImpl = vi.fn(async (method: string) => {
        if (method === "workspaces.list") {
          return { workspaces: [{ canonical_path: workspace, state: "active" }] };
        }
        if (method === "session.context") {
          return { session_context: {
            session_id: "sess_active",
            session_state: sessionState,
            client_type: "pi",
            client_session_key: "pi_session_active",
            runtime_instance_id: "rtinst_active",
          } };
        }
        return Promise.reject(new Error(`unexpected ${method}`));
      });
      const { handlers, reported } = install({ request: requestImpl as any });

      await handlers.session_start({ reason: "startup" }, {
        sessionManager: { getSessionId: () => "pi_session_active", getCwd: () => workspace },
      });

      expect(requestImpl).toHaveBeenCalledTimes(2);
      expect(reported).toEqual([]);
    },
  );

  test("a starting pontia session allows its native client to finish binding and report ready", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: {
          session_id: "sess_starting",
          session_state: "starting",
          client_type: "pi",
          runtime_instance_id: "rtinst_starting",
        } };
      }
      if (method === "runtime.register") {
        return {
          session: { session_id: "sess_starting" },
          runtime: {
            runtime_instance_id: "rtinst_bound",
          },
        };
      }
      return Promise.reject(new Error(`unexpected ${method}`));
    });
    const { handlers, reported } = install({ request: requestImpl as any });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_starting", getCwd: () => workspace },
    });

    expect(requestImpl).toHaveBeenCalledTimes(3);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_starting",
      data: { runtime_instance_id: "rtinst_bound" },
    });
  });

  test("manual session_start startup immediately reattaches when client session already has a pontia binding", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string, params?: any) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: {
          session_id: "sess_existing",
          session_state: "exited",
          client_type: "pi",
          client_session_key: "pi_session_resumed",
          runtime_instance_id: "rtinst_exited",
        } };
      }
      if (method === "runtime.register") {
        expect(params).toMatchObject({
          client_session_key: "pi_session_resumed",
          client_session_file: "/tmp/pi/resumed.jsonl",
          tmux: { socket_path: "/tmp/tmux-1000/default", pane_id: "%42" },
        });
        return {
          session: { session_id: "sess_existing" },
          runtime: {
            runtime_instance_id: "rtinst_reattached",
          },
        };
      }
      return Promise.reject(new Error(`unexpected ${method}`));
    });
    const { handlers, reported } = install({
      env: {
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_resumed",
        getSessionFile: () => "/tmp/pi/resumed.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(requestImpl).toHaveBeenCalledTimes(3);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({
      session_id: "sess_existing",
      data: {
        runtime_instance_id: "rtinst_reattached",
        client_session_key: "pi_session_resumed",
        client_session_file: "/tmp/pi/resumed.jsonl",
      },
    });
  });

  test("manual new session is not persisted until its first prompt starts", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: null };
      }
      if (method === "runtime.register") {
        return {
          session: { session_id: "sess_manual" },
          runtime: { runtime_instance_id: "rtinst_manual" },
        };
      }
      return Promise.reject(new Error("unexpected"));
    });
    const { handlers, reported } = install({
      env: {
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
      },
      request: requestImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: {
        getSessionId: () => "pi_session_manual",
        getSessionFile: () => "/tmp/pi/session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(requestImpl).toHaveBeenCalledTimes(2);
    expect(requestImpl).not.toHaveBeenCalledWith("runtime.register", expect.anything());
    expect(reported).toEqual([]);

    await handlers.before_agent_start({ prompt: "first message", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {
      sessionManager: {
        getSessionId: () => "pi_session_manual",
        getSessionFile: () => "/tmp/pi/session.jsonl",
        getSessionDir: () => "/tmp/pi",
        getCwd: () => workspace,
      },
    });

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "turn.started"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_manual",
      data: { input_summary: "first message" },
    });
  });

  test("session_start skips manual binding when current workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [] };
      }
      return Promise.reject(new Error("unexpected"));
    });
    const logDiagnostic = vi.fn(async () => undefined);
    const { handlers, reported } = install({
      env: {
      },
      request: requestImpl as any,
      logDiagnostic,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_manual", getCwd: () => workspace },
    });
    await handlers.before_agent_start({ prompt: "typed in tui", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, {});

    expect(requestImpl).toHaveBeenCalledTimes(1);
    expect(reported).toEqual([]);
    expect(logDiagnostic).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ code: "workspace_not_active" }));
  });

  test("session_start skips managed ready reporting when current workspace is not active", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      expect(method).toBe("workspaces.list");
      return { workspaces: [{ canonical_path: workspace, state: "deleted" }] };
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_ready",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });

    expect(reported).toEqual([]);
  });

  test("session_start confirms binding without HTTP configuration", async () => {
    const root = await tempDir();
    const workspace = await realpath(await tempDir());
    await mkdir(join(root, ".pontia"), { recursive: true });

    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: {
          session_id: "sess_discovered",
          session_state: "exited",
          client_type: "pi",
          runtime_instance_id: "rtinst_old",
        } };
      }
      if (method === "runtime.register") {
        return {
          session: { session_id: "sess_discovered" },
          runtime: { runtime_instance_id: "rtinst_discovered" },
        };
      }
      return Promise.reject(new Error("unexpected"));
    });
    const { handlers, reported } = install({ env: { PONTIA_HOME: join(root, ".pontia") }, request: requestImpl as any });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_discovered", getCwd: () => workspace },
    });

    expect(requestImpl).toHaveBeenCalledTimes(3);
    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
  });

  test("session_start uses PONTIA_HOME loaded after extension registration", async () => {
    const root = await tempDir();
    const workspace = await realpath(await tempDir());
    const stableHome = join(root, ".pontia-stable");
    await mkdir(stableHome, { recursive: true });

    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: null };
      }
      if (method === "runtime.register") {
        return {
          session: { session_id: "sess_late_env" },
          runtime: { runtime_instance_id: "rtinst_late_env" },
        };
      }
      return Promise.reject(new Error("unexpected"));
    });
    const { handlers, reported, env } = install({
      env: { PONTIA_HOME: join(root, ".pontia") },
      request: requestImpl as any,
      loadContext: vi.fn(async () => ({
        ok: false as const,
        reason: "current turn claim unavailable",
        logFile: "fallback/pi-hook.log",
        silent: true,
      })),
    });

    env.PONTIA_HOME = stableHome;
    const sessionManager = { getSessionId: () => "pi_session_late_env", getCwd: () => workspace };
    await handlers.session_start({ reason: "startup" }, { sessionManager });
    await handlers.before_agent_start({ prompt: "first message", systemPrompt: "Base prompt" }, {});
    await handlers.agent_start({}, { sessionManager });

    expect(requestImpl).toHaveBeenCalledTimes(4);
    expect(reported.map((event) => event.type)).toEqual(["session.ready", "turn.started"]);
    expect(reported[0]).toMatchObject({ session_id: "sess_late_env" });
  });

  test("agent_start does not claim a turn from tmux marker identity alone", async () => {
    const dir = await tempDir();
    const requestImpl = vi.fn(async () =>
      ({
            current_turn: {
              session_id: "sess_consumed",
              input: "from web",
              inbox_message_id: "msg_consumed",
              runtime_instance_id: "rtinst_consumed",
              client_type: "pi",
            },
          }),
    );

    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_consumed",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_consumed",
        PONTIA_HOME: dir,
      },
      request: requestImpl as any,
      loadContext: (env, sessionContext) => loadTurnContext(env, { connection: { request: requestImpl as any }, sessionContext }),
    });

    await handlers.agent_start({}, {});

    expect(reported).toEqual([]);
    expect(requestImpl).not.toHaveBeenCalled();
  });

  test("manual tui agent_start uses the bound session when current-turn context is absent", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      return {
        session: { session_id: "sess_bound" },
        runtime: {
          runtime_instance_id: "rtinst_bound",
        },
      };
    });
    const { handlers, reported } = install({
      env: {
      },
      request: requestImpl as any,
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
      data: { runtime_instance_id: "rtinst_bound", input_summary: "typed in tui" },
    });
  });

  test("session_start fork binds a new pontia session with parent lineage", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string, params?: any) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      expect(method).toBe("runtime.register");
      const body = params;
      expect(body).toMatchObject({
          client_session_key: "pi_child",
        start_kind: "fork",
        parent_session_id: "sess_parent",
      });
      expect(body).not.toHaveProperty("runtime_instance_id");
      return {
        session: { session_id: "sess_child" },
        runtime: {
          runtime_instance_id: "rtinst_child",
        },
      };
    });
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_parent",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_parent",
      },
      request: requestImpl as any,
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
    expect(reported[1]).toMatchObject({ session_id: "sess_child", data: { runtime_instance_id: "rtinst_child" } });
    expect(reported[2]).toMatchObject({ session_id: "sess_child", data: { input_summary: "fork prompt" } });
  });

  test("session_start resume immediately reattaches when switched client session has a pontia binding", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async (method: string, params?: any) => {
      if (method === "workspaces.list") {
        return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      }
      if (method === "session.context") {
        return { session_context: {
          session_id: "sess_resume",
          session_state: "starting",
          client_type: "pi",
          runtime_instance_id: "rtinst_resume",
        } };
      }
      if (method === "runtime.register") {
        expect(params).toMatchObject({
          client_session_key: "pi_session_resume",
          runtime_instance_id: "rtinst_resume",
        });
        return {
          session: { session_id: "sess_resume" },
          runtime: {
            runtime_instance_id: "rtinst_resume",
          },
        };
      }
      return Promise.reject(new Error(`unexpected ${method}`));
    });
    const { handlers, reported } = install({
      env: {
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "resume" }, {
      sessionManager: { getSessionId: () => "pi_session_resume", getCwd: () => workspace },
    });

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
    expect(reported[0]).toMatchObject({ session_id: "sess_resume" });
  });

  test("session_start does not report ready when binding confirmation is unavailable", async () => {
    const requestImpl = vi.fn(async () => Promise.reject(new Error("unexpected")));
    const { handlers, reported } = install({
      env: { PONTIA_SESSION_ID: "sess_partial" },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_partial", getCwd: () => "/workspace" },
    });

    expect(requestImpl).toHaveBeenCalledTimes(1);
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

  test("registers pi lifecycle handlers without custom tools", () => {
    const { pi } = fakePi();
    createPontiaPiExtension(pi as any, {
      env: {
        PONTIA_HOME: defaultPontiaHome,
        TMUX: "/tmp/tmux-1000/default,2071,502",
        TMUX_PANE: "%42",
      },
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
    const requestImpl = vi.fn(async () => ({ workspaces: [{ canonical_path: workspace, state: "active" }] }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: "quit" }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "session.exited"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_exit",
      data: { reason: "quit", runtime_instance_id: "rtinst_1" },
    });
  });

  test.each(["new", "resume", "fork"])("session_shutdown %s reports session exited for managed runtime", async (shutdownReason) => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async () => ({ workspaces: [{ canonical_path: workspace, state: "active" }] }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: shutdownReason }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready", "session.exited"]);
    expect(reported[1]).toMatchObject({
      session_id: "sess_exit",
      data: { reason: shutdownReason, runtime_instance_id: "rtinst_1" },
    });
  });

  test("session_shutdown reload does not report session exited", async () => {
    const workspace = await realpath(await tempDir());
    const requestImpl = vi.fn(async () => ({ workspaces: [{ canonical_path: workspace, state: "active" }] }));
    const { handlers, reported } = install({
      env: {
        PONTIA_SESSION_ID: "sess_exit",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
    });

    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "pi_session_1", getCwd: () => workspace },
    });
    await handlers.session_shutdown({ reason: "reload" }, {});

    expect(reported.map((event) => event.type)).toEqual(["session.ready"]);
  });

  test("does not select an execution profile from tmux marker identity alone", async () => {
    const requestImpl = vi.fn(async (method: string) => {
      if (method === "session.get") {
        return { session: { execution_profile_id: "reviewer", execution_profile_version: "1" } };
      }
      if (method === "profile.get") {
        return { agent_profile: { system_prompt_template: "Reviewer instructions" } };
      }
      return { session_context: null };
    });
    const { handlers } = install({
      env: {
        PONTIA_SESSION_ID: "sess_1",
        PONTIA_RUNTIME_INSTANCE_ID: "rtinst_1",
      },
      request: requestImpl as any,
    });

    const result = await handlers.before_agent_start({ systemPrompt: "Base prompt" }, {});

    expect(result).toEqual({ systemPrompt: "Base prompt" });
    expect(requestImpl).not.toHaveBeenCalled();
  });

  test.each(["Reviewer instructions", null, "error"])("uses the bound session profile prompt %s", async (prompt) => {
    const workspace = await realpath(await tempDir());
    const request = vi.fn(async (method: string, params: object) => {
      if (method === "workspaces.list") return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      if (method === "session.get") {
        expect(params).toEqual({ session_id: "sess_profile" });
        return { session: { execution_profile_id: "reviewer", execution_profile_version: "1" } };
      }
      expect(method).toBe("profile.get");
      expect(params).toEqual({ profile_id: "reviewer", version: "1" });
      if (prompt === "error") throw new Error("profile unavailable");
      return { agent_profile: { system_prompt_template: prompt } };
    });
    const { handlers } = install({
      env: { PONTIA_SESSION_ID: "sess_profile", PONTIA_RUNTIME_INSTANCE_ID: "rt_profile" },
      request,
    });
    await handlers.session_start({ reason: "startup" }, {
      sessionManager: { getSessionId: () => "native", getCwd: () => workspace },
    });
    expect(await handlers.before_agent_start({ systemPrompt: "Base prompt" }, {})).toEqual({
      systemPrompt: prompt && prompt !== "error" ? `Base prompt\n\n${prompt}` : "Base prompt",
    });
    expect(request).toHaveBeenCalledWith("profile.get", { profile_id: "reviewer", version: "1" });
  });

  test("reports context usage when a hook event exposes valid usage", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ context_usage: { used_tokens: 2, max_tokens: 8, usage_ratio: 0.25, confidence: "estimated" } }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.context_usage_updated"]);
    expect(reported[1]).toMatchObject({
      turn_id: "turn_server_1",
      data: {
        context_usage: {
          used_tokens: 2,
          max_tokens: 8,
          usage_ratio: 0.25,
          confidence: "estimated",
        },
      },
    });
  });

  test("reports context usage without overwriting the separately confirmed session model", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { text_delta: "hello" } }, {
      model: { id: "gpt-5.5" },
      getContextUsage: () => ({ tokens: 6037, contextWindow: 128000, percent: 4.716 }),
    });

    expect(reported[1].data).not.toHaveProperty("model");
    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.context_usage_updated"]);
    expect(reported[1]).toMatchObject({
      data: {
        context_usage: {
          used_tokens: 6037,
          max_tokens: 128000,
          remaining_tokens: 121963,
          usage_ratio: 0.04716,
          confidence: "estimated",
        },
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
    expect(reported[0].data).toEqual({
      runtime_instance_id: "rtinst_1",
      input_summary: undefined,
      previous_leaf_id: null,
    });
    expect(reported[1].data).toEqual({ output_summary: "hello world" });
    expect(reported[3]).toMatchObject({ data: { reason: "final" } });
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
    expect(report.mock.calls[0]?.[1].data).toMatchObject({
      previous_leaf_id: "entry_before_turn",
    });
    releaseStarted();
    await starting;

    leafId = "entry_after_turn";
    await handlers.agent_end({ messages: [] }, hookContext);

    const completed = report.mock.calls.find(([, event]) => event.type === "turn.completed")?.[1];
    expect(completed?.data).toMatchObject({
      terminal_leaf_id: "entry_after_turn",
    });
    expect(observations.indexOf("leaf:entry_after_turn")).toBeLessThan(observations.indexOf("report:turn.completed"));
  });

  test("captures a content-free Pi context path before reporting turn.started", async () => {
    const observations: string[] = [];
    const report = vi.fn(async (_ctx: TurnContext, event: InternalEvent) => {
      observations.push(`report:${event.type}`);
      return true;
    });
    const { handlers } = install({ makeReporter: vi.fn(() => ({ report })) });
    const hookContext = {
      sessionManager: {
        getLeafId: () => "assistant_1",
        getBranch: () => {
          observations.push("branch");
          return [
            {
              type: "message",
              id: "user_1",
              parentId: null,
              timestamp: "2026-07-16T00:00:00Z",
              message: { role: "user", content: "secret prompt" },
            },
            {
              type: "compaction",
              id: "compact_1",
              parentId: "user_1",
              timestamp: "2026-07-16T00:00:01Z",
              summary: "secret summary",
              firstKeptEntryId: "user_1",
              tokensBefore: 10,
            },
            {
              type: "message",
              id: "assistant_1",
              parentId: "compact_1",
              timestamp: "2026-07-16T00:00:02Z",
              message: {
                role: "assistant",
                content: [{ type: "text", text: "secret answer" }],
                toolCalls: [{ name: "secret-tool", arguments: { token: "secret" } }],
              },
            },
          ];
        },
      },
    };

    await handlers.agent_start({}, hookContext);

    expect(observations).toEqual(["branch", "report:turn.started"]);
    expect(report.mock.calls[0]?.[1].data).toMatchObject({
      topology_context: {
        entries: [
          { id: "user_1", kind: "user_message" },
          { id: "compact_1", kind: "compaction" },
          { id: "assistant_1", kind: "assistant_message" },
        ],
      },
    });
    expect(JSON.stringify(report.mock.calls[0]?.[1].data)).not.toMatch(
      /secret prompt|secret summary|secret answer|secret-tool|token/,
    );
  });

  test("uses assistant message_end full text without TUI parsing", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.message_end({ message: { role: "assistant", content: [{ type: "text", text: "final answer" }] } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "session.message_updated", "turn.output", "turn.completed", "session.message_updated"]);
    expect(reported[1]).toMatchObject({ data: { reason: "append" } });
    expect(reported[2].data).toEqual({ output_summary: "final answer" });
    expect(reported[4]).toMatchObject({ data: { reason: "final" } });
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
    expect(reported.slice(1).map((event) => event.data)).toEqual([
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
    expect(reported[7]).toMatchObject({ data: { reason: "final" } });
  });

  test("publishes assistant deltas and only complete tool calls to the turn live stream", async () => {
    const appendText = vi.fn();
    const appendToolCall = vi.fn();
    const close = vi.fn(async () => undefined);
    const { handlers } = install({
      makeLiveOutputPublisher: vi.fn(() => ({ appendText, appendToolCall, close })),
    });

    await handlers.agent_start({}, {});
    await handlers.message_update({ assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "hello" } }, {});
    await handlers.message_end({ message: { role: "assistant", content: "hello" } }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "toolcall_delta", contentIndex: 0, delta: "{\"path\":" } }, {});
    expect(appendToolCall).not.toHaveBeenCalled();
    await handlers.message_update({
      assistantMessageEvent: {
        type: "toolcall_end",
        contentIndex: 1,
        toolCall: { id: "call_1", name: "read", arguments: { path: "README.md" } },
      },
    }, {});
    await handlers.message_update({ assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: "done" } }, {});
    await handlers.agent_end({ messages: [] }, {});

    expect(appendText.mock.calls).toEqual([["hello"], ["done"]]);
    expect(appendToolCall).toHaveBeenCalledWith({
      callId: "call_1",
      toolName: "read",
      arguments: { path: "README.md" },
    });
    expect(close).toHaveBeenCalledOnce();
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
    expect(reported.slice(1).map((event) => event.data)).toEqual([
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
      { reason: "update" },
    ]);
  });

  test("uses a fresh backend-provided canonical turn id for each real pi agent_start", async () => {
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
    expect(started.every((event) => event.turn_id === undefined)).toBe(true);
    expect(reported.filter((event) => event.type === "turn.output").map((event) => event.turn_id)).toEqual([
      "turn_server_1",
      "turn_server_2",
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

  test("reports turn.interrupted when Pi ends with an aborted assistant message", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.agent_end({
      messages: [{
        role: "assistant",
        content: [],
        stopReason: "aborted",
        errorMessage: "Request was aborted",
      }],
    }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.interrupted", "session.message_updated"]);
    expect(reported[1].data).toEqual({ terminal_leaf_id: null });
    expect(reported[2]).toMatchObject({ data: { reason: "final" } });
  });

  test("reports turn.interrupted when Pi surfaces an aborted operation as an error", async () => {
    const { handlers, reported } = install();
    const abortController = new AbortController();
    abortController.abort();

    await handlers.agent_start({}, {});
    await handlers.agent_end({
      messages: [{
        role: "assistant",
        content: [],
        stopReason: "error",
        errorMessage: "This operation was aborted",
      }],
    }, { signal: abortController.signal });

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.interrupted", "session.message_updated"]);
    expect(reported[1].data).toEqual({ terminal_leaf_id: null });
  });

  test("reports turn.failed when Pi ends with an errored assistant message", async () => {
    const { handlers, reported } = install();

    await handlers.agent_start({}, {});
    await handlers.agent_end({
      messages: [{
        role: "assistant",
        content: [],
        stopReason: "error",
        errorMessage: "model failed",
      }],
    }, {});

    expect(reported.map((event) => event.type)).toEqual(["turn.started", "turn.failed", "session.message_updated"]);
    expect(reported[1].data).toEqual({
      failure_message: "model failed",
      terminal_leaf_id: null,
    });
    expect(reported[2]).toMatchObject({ data: { reason: "final" } });
  });
});
