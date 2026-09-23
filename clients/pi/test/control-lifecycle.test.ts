import { realpath, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test, vi } from "vitest";
import { createPontiaPiExtension } from "../src/index.js";
import type { InternalEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

test("session switches register a fresh connection and failed registration never reports ready", async () => {
  const root = await tempDir("pc-");
  const workspace = await realpath(root);
  await writeFile(join(root, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
  const handlers: Record<string, (event: any, context?: any) => Promise<void>> = {};
  const events: InternalEvent[] = [];
  const registered: string[] = [];
  const closes: Array<ReturnType<typeof vi.fn>> = [];
  createPontiaPiExtension({ on(name: string, handler: any) { handlers[name] = handler; }, registerCommand() {} } as any, {
    env: { PONTIA_HOME: root, TMUX: "/unused/tmux,1,1", TMUX_PANE: "%1" },
    fetch: vi.fn(async () => Response.json({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } })) as typeof fetch,
    connectPi: async () => {
      const close = vi.fn(async () => {}); closes.push(close);
      return { close, registered(identity) { registered.push(identity.sessionId); }, async request(method, params) {
        if (method === "session.context") return { session_context: null };
        const id = (params as any).binding.client_session_key;
        if (id === "failed") throw new Error("Registration rejected");
        return { session: { session_id: `sess_${id}` }, runtime: { runtime_instance_id: `rt_${id}`, internal_event_url: "http://localhost/internal/v1/events" } };
      } };
    },
    isManagedPane: async () => true,
    makeReporter: () => ({ report: async (_context, event) => { events.push(event); return true; } }),
    logDiagnostic: vi.fn(async () => {}),
  });
  const context = (id: string) => ({ mode: "tui", sessionManager: { getSessionId: () => id, getSessionFile: () => join(root, `${id}.jsonl`), getCwd: () => workspace } });
  await handlers.session_start({ reason: "startup" }, context("one"));
  await handlers.session_shutdown({ reason: "new" });
  expect(closes[0]).toHaveBeenCalledOnce();
  await handlers.session_start({ reason: "new" }, context("two"));
  await handlers.session_shutdown({ reason: "new" });
  await handlers.session_start({ reason: "new" }, context("failed"));
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
  await writeFile(join(root, "config.toml"), 'bind_addr = "localhost:80"\nexternal_api_token = "token"\n');
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
        if (request.method === "runtime.register") {
          registrations += 1;
          if (registrations === 1) { socket.destroy(); return; }
          managed = true;
          result = { session: { session_id: "sess_recovered" }, runtime: { runtime_instance_id: "rt_recovered", internal_event_url: "http://localhost/internal/v1/events" } };
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
    fetch: vi.fn(async () => Response.json({ data: { workspaces: [{ canonical_path: workspace, state: "active" }] } })) as typeof fetch,
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
