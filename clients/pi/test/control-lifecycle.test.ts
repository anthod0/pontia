import { realpath } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import type { LifecycleControl } from "../src/control-socket.js";
import type { InternalEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

test("session switches register a fresh connection and failed registration never reports ready", async () => {
  const root = await tempDir("pc-");
  const workspace = await realpath(root);
  const handlers: Record<string, (event: any, context?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  const registered: string[] = [];
  const controls: LifecycleControl[] = [];
  const calls: string[] = [];
  const closes: Array<ReturnType<typeof vi.fn>> = [];
  createPontiaPiExtension({ on(name: string, handler: any) { handlers[name] = handler; }, registerCommand() {} } as any, {
    env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    connectPi: async (_home, _error, _submit, _models, _replay, lifecycle) => {
      controls.push(lifecycle!);
      expect(() => lifecycle!.interrupt()).toThrow();
      expect(() => lifecycle!.shutdown()).toThrow();
      const close = vi.fn(async () => {}); closes.push(close);
      return { close, registered(identity) { registered.push(identity.sessionId); }, async request(method, params) {
        if (method === "event.report") {
          expect(close).not.toHaveBeenCalled();
          events.push((params as any).event);
          return { accepted: true };
        }
        if (method === "workspaces.list") return { workspaces: [{ canonical_path: workspace, state: "active" }] };
        if (method === "session.context") return { session_context: null };
        const id = (params as any).binding.client_session_key;
        if (id === "failed") throw new Error("Registration rejected");
        return { session: { session_id: `sess_${id}` }, runtime: { runtime_instance_id: `rt_${id}` } };
      } };
    },
    isManagedPane: async () => true,
    logDiagnostic: vi.fn(async () => {}),
  });
  const context = (id: string) => ({ abort: () => calls.push(`${id}:abort`), shutdown: () => calls.push(`${id}:shutdown`), mode: "tui", sessionManager: { getSessionId: () => id, getSessionFile: () => join(root, `${id}.jsonl`), getCwd: () => workspace } });
  await handlers.session_start({ reason: "startup" }, context("one"));
  controls[0].interrupt();
  controls[0].shutdown();
  expect(calls).toEqual(["one:abort", "one:abort", "one:shutdown"]);
  expect(events.map((event) => event.type)).toEqual(["session.ready"]);
  await handlers.session_shutdown({ reason: "new" });
  expect(closes[0]).toHaveBeenCalledOnce();
  await handlers.session_start({ reason: "new" }, context("two"));
  expect(() => controls[0].interrupt()).toThrow();
  expect(() => controls[0].shutdown()).toThrow();
  controls[1].shutdown();
  expect(calls.slice(3)).toEqual(["two:abort", "two:shutdown"]);
  await handlers.session_shutdown({ reason: "new" });
  await handlers.session_start({ reason: "new" }, context("failed"));
  expect(() => controls[2].shutdown()).toThrow();
  expect(registered).toEqual(["sess_one", "sess_two"]);
  expect(events.map((event) => event.type)).toEqual(["session.ready", "session.exited", "session.ready", "session.exited"]);
  await handlers.session_shutdown({ reason: "quit" });
});

test("a disconnected deferred registration can initialize on the next manual turn", async () => {
  const { mkdir } = await import("node:fs/promises");
  const { createServer } = await import("node:net");
  const { piSocketPath } = await import("../src/control-socket.js");
  const root = await tempDir("pd-");
  const workspace = await realpath(root);
  await mkdir(join(root, "state/pi"), { recursive: true });
  let registrations = 0;
  let managed = false;
  const sockets = new Set<import("node:net").Socket>();
  const server = createServer((socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => {});
    let buffered = "";
    socket.on("data", (chunk) => {
      buffered += chunk.toString();
      for (;;) {
        const newline = buffered.indexOf("\n");
        if (newline < 0) break;
        const request = JSON.parse(buffered.slice(0, newline));
        buffered = buffered.slice(newline + 1);
        let result: unknown = { session_context: null };
        if (request.method === "workspaces.list") result = { workspaces: [{ canonical_path: workspace, state: "active" }] };
        if (request.method === "runtime.register") {
          registrations += 1;
          if (registrations === 1) { socket.destroy(); return; }
          managed = true;
          result = { session: { session_id: "sess_recovered" }, runtime: { runtime_instance_id: "rt_recovered" } };
        }
        socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result })}\n`);
      }
    });
  });
  await new Promise<void>((resolve) => server.listen(piSocketPath(root), resolve));
  const handlers: Record<string, (event: any, context?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  createPontiaPiExtension({ on(name: string, handler: any) { handlers[name] = handler; }, registerCommand() {} } as any, {
    env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    isManagedPane: async () => managed,
    loadContext: async () => ({ ok: false, silent: true, reason: "unbound", logFile: join(root, "hook.log") }),
    makeReporter: () => ({ report: async (_context, event) => { events.push(event); return { accepted: true, turnId: "turn_recovered" }; } }),
    logDiagnostic: vi.fn(async () => {}),
  });
  const context = { mode: "tui", sessionManager: { getSessionId: () => "native", getSessionFile: () => join(root, "pi.jsonl"), getCwd: () => workspace } };
  try {
    await handlers.session_start({ reason: "startup" }, context);
    await handlers.agent_start({}, context);
    expect(registrations).toBe(1);
    expect(events).toEqual([]);
    await handlers.agent_start({}, context);
    expect(registrations).toBe(2);
    expect(events.map((event) => event.type)).toEqual(["session.ready", "turn.started"]);
    expect(events[1].session_id).toBe("sess_recovered");
  } finally {
    await handlers.session_shutdown({ reason: "quit" });
    for (const socket of sockets) socket.destroy();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
});

test("the default extension streams over its registered connection and closes the canonical turn stream", async () => {
  const root = await tempDir("pl-");
  const workspace = await realpath(root);
  const handlers: Record<string, (event: any, context?: any) => Promise<void>> = {};
  const calls: Array<{ method: string; params: any }> = [];
  const connect = vi.fn(async () => ({
    registered() {}, async close() {},
    async request(method: string, params: object) {
      calls.push({ method, params });
      if (method === "workspaces.list") return { workspaces: [{ canonical_path: workspace, state: "active" }] };
      if (method === "session.context") return { session_context: null };
      if (method === "runtime.register") return {
        session: { session_id: "sess_live" },
        runtime: { runtime_instance_id: "rt_live" },
      };
      if (method === "liveOutput.publish") return {
        accepted: true, accepted_sequence: (params as any).sequence, resync_required: false,
      };
      expect(method).toBe("event.report");
      return { accepted: true, turn_id: "turn_canonical" };
    },
  }));
  createPontiaPiExtension({ on(name: string, handler: any) { handlers[name] = handler; }, registerCommand() {} } as any, {
    env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    connectPi: connect,
    isManagedPane: async () => true,
    loadContext: async () => ({ ok: true, logFile: join(root, "hook.log"), context: {
      sessionId: "sess_live", runtimeInstanceId: "rt_live", clientType: "pi",
    } }),
  });
  const context = { mode: "tui", sessionManager: {
    getSessionId: () => "native", getSessionFile: () => join(root, "pi.jsonl"), getCwd: () => workspace,
  } };
  try {
    await handlers.session_start({ reason: "startup" }, context);
    await handlers.agent_start({}, context);
    await handlers.message_update({ assistantMessageEvent: { type: "text_delta", delta: "hello" } }, context);
    await vi.waitFor(() => expect(calls.filter((call) => call.method === "liveOutput.publish")).toHaveLength(1));
    await handlers.agent_end({ messages: [] }, context);
    const liveCalls = calls.filter((call) => call.method === "liveOutput.publish");
    expect(liveCalls.map((call) => call.params)).toEqual([
      expect.objectContaining({ session_id: "sess_live", runtime_instance_id: "rt_live", turn_id: "turn_canonical",
        type: "snapshot", sequence: 1, items: [{ kind: "assistant_text", item_id: "text_1", text: "hello" }] }),
      expect.objectContaining({ type: "stream_closed", sequence: 2 }),
    ]);
    expect(calls.indexOf(liveCalls[0])).toBeLessThan(calls.findIndex((call) => call.params.event?.type === "turn.completed"));
    expect(connect).toHaveBeenCalledOnce();
  } finally {
    await handlers.session_shutdown({ reason: "quit" }, context);
  }
});
