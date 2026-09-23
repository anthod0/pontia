import { mkdir } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { join } from "node:path";
import { onTestFinished, expect, test, vi } from "vitest";
import { connectPi, piSocketPath, MAX_RPC_FRAME_BYTES, RpcError } from "../src/control-socket.js";
import { EventReporter } from "../src/reporter.js";
import { buildTurnStartedEvent } from "../src/events.js";
import { tempDir } from "./temp-dir.js";

async function server(handler: (socket: Socket, message: any) => void) {
  const root = await tempDir("pr-");
  await mkdir(join(root, "state/pi"), { recursive: true });
  const sockets = new Set<Socket>();
  const listener = createServer((socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
    socket.on("error", () => {});
    let pending = "";
    socket.on("data", (chunk) => {
      pending += chunk.toString();
      for (;;) {
        const index = pending.indexOf("\n");
        if (index < 0) break;
        const line = pending.slice(0, index); pending = pending.slice(index + 1);
        handler(socket, JSON.parse(line));
      }
    });
  });
  await new Promise<void>((resolve) => listener.listen(piSocketPath(root), resolve));
  onTestFinished(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise<void>((resolve) => listener.close(() => resolve()));
  });
  return root;
}

function reply(socket: Socket, id: unknown, result: unknown) { socket.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`); }

test("dispatches daemon requests while a registration request is awaiting its reply", async () => {
  let registrationId: unknown;
  const root = await server((socket, message) => {
    if (message.method === "runtime.register") {
      registrationId = message.id;
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "daemon:1", method: "submit", params: { input: "你好", inbox_message_id: "msg_1" } })}\n`);
    } else {
      expect(message.id).toBe("daemon:1");
      expect(message.result).toEqual({ accepted: true });
      reply(socket, registrationId, { session_id: "s", runtime_instance_id: "r" });
    }
  });
  const submit = vi.fn();
  const client = await connectPi(root, () => {}, submit);
  onTestFinished(() => client.close());
  await expect(client.request("runtime.register", {})).resolves.toEqual({ session_id: "s", runtime_instance_id: "r" });
  expect(submit).toHaveBeenCalledWith({ input: "你好", inboxMessageId: "msg_1" });
});

test("reconnect attaches the confirmed identity without repeating registration", async () => {
  const methods: string[] = [];
  const root = await server((socket, message) => {
    methods.push(message.method);
    if (message.method === "runtime.register") reply(socket, message.id, {});
    else if (message.method === "break") socket.destroy();
    else {
      expect(message.method).toBe("runtime.attach");
      expect(message.params).toMatchObject({ session_id: "s", runtime_instance_id: "r", client_session_key: "native" });
      reply(socket, message.id, { session_id: "s", runtime_instance_id: "r" });
    }
  });
  const client = await connectPi(root, () => {}, () => {});
  onTestFinished(() => client.close());
  await client.request("runtime.register", {});
  client.registered({ sessionId: "s", runtimeInstanceId: "r", clientSessionKey: "native" });
  await expect(client.request("break", {})).rejects.toThrow();
  await vi.waitFor(() => expect(methods).toEqual(["runtime.register", "break", "runtime.attach"]));
});

test.each(["wrong-id", "both", "invalid-json", "oversized"])("rejects %s frames and fails pending calls", async (kind) => {
  const root = await server((socket, message) => {
    const frame = kind === "wrong-id" ? JSON.stringify({ jsonrpc: "2.0", id: "wrong", result: {} })
      : kind === "both" ? JSON.stringify({ jsonrpc: "2.0", id: message.id, result: {}, error: { code: 1, message: "bad" } })
      : kind === "oversized" ? "x".repeat(MAX_RPC_FRAME_BYTES + 1) : "not-json";
    socket.write(`${frame}\n`);
  });
  const client = await connectPi(root, () => {}, () => {});
  onTestFinished(() => client.close());
  await expect(client.request("runtime.register", {})).rejects.toThrow();
});

test("correlates replies that arrive in reverse order", async () => {
  const pending: any[] = [];
  const root = await server((socket, message) => {
    pending.push(message);
    if (pending.length === 2) for (const request of pending.reverse()) reply(socket, request.id, request.method);
  });
  const client = await connectPi(root, () => {}, () => {});
  onTestFinished(() => client.close());
  await expect(Promise.all([client.request("one", {}), client.request("two", {})])).resolves.toEqual(["one", "two"]);
});


test("lost acknowledgements retry only the failure notification over a fresh connection", async () => {
  const methods: string[] = [];
  const root = await server((socket, message) => {
    if (!message.method) {
      expect(message.id).toBe("daemon:submit");
      expect(message.result).toEqual({ accepted: true });
      socket.destroy();
      return;
    }
    methods.push(message.method);
    if (message.method === "event.report") {
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "daemon:submit", method: "submit", params: { input: "next" } })}\n`);
    } else if (message.method === "runtime.attach") {
      setTimeout(() => reply(socket, message.id, { session_id: "s", runtime_instance_id: "r" }), 400);
    } else {
      expect(message.params.client_session_key).toBe("native");
      if (methods.filter((method) => method === "turn.startFailure").length === 1) socket.destroy();
      else reply(socket, message.id, { accepted: true });
    }
  });
  const submit = vi.fn();
  const client = await connectPi(root, () => {}, submit);
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeInstanceId: "r", clientSessionKey: "native" });
  const reporter = new EventReporter({ connection: client, logFile: join(root, "hook.log") });
  const context = { sessionId: "s", runtimeInstanceId: "r", clientType: "pi" as const, internalEventUrl: "unused" };
  expect(await reporter.report(context, buildTurnStartedEvent(context))).toEqual({ accepted: false });
  expect(methods.filter((method) => method !== "runtime.attach")).toEqual(["event.report", "turn.startFailure", "turn.startFailure"]);
  expect(submit).toHaveBeenCalledWith({ input: "next", inboxMessageId: undefined });
});

test("reports large facts without treating normal socket buffering as failure", async () => {
  const data = "界".repeat(40_000);
  const root = await server((socket, message) => {
    expect(message.params.event.data.input_summary).toBe(data);
    reply(socket, message.id, { accepted: true, turn_id: "turn_1" });
  });
  const client = await connectPi(root, () => {}, () => {});
  onTestFinished(() => client.close());
  await expect(client.request("event.report", { event: { data: { input_summary: data } } })).resolves.toMatchObject({ accepted: true });
});

test("new event reports wait for reconnect attachment before they are sent", async () => {
  let attached = false;
  const methods: string[] = [];
  const root = await server((socket, message) => {
    methods.push(message.method);
    if (message.method === "break") { socket.destroy(); return; }
    if (message.method === "runtime.attach") {
      setTimeout(() => {
        attached = true;
        reply(socket, message.id, { session_id: "s", runtime_instance_id: "r" });
      }, 100);
      return;
    }
    expect(attached).toBe(true);
    reply(socket, message.id, { accepted: true });
  });
  const client = await connectPi(root, () => {}, () => {});
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeInstanceId: "r", clientSessionKey: "native" });
  await expect(client.request("break", {})).rejects.toThrow();
  await vi.waitFor(() => expect(methods).toContain("runtime.attach"));
  await expect(client.request("event.report", {})).resolves.toEqual({ accepted: true });
  expect(methods).toEqual(["break", "runtime.attach", "event.report"]);
});

test("model control can report a confirmed fact before replying and refreshes it after reconnect", async () => {
  const responses: any[] = [];
  const methods: string[] = [];
  const root = await server((socket, message) => {
    if (!message.method) { responses.push(message); return; }
    methods.push(message.method);
    if (message.method === "begin") {
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "list", method: "models.list" })}\n`);
      socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "set", method: "model.set", params: { model: "provider/model" } })}\n`);
    }
    if (message.method === "break") { socket.destroy(); return; }
    reply(socket, message.id, message.method === "runtime.attach" ? { session_id: "s", runtime_instance_id: "r" } : { accepted: true });
  });
  const model = { id: "provider/model", name: "Model", description: "provider" };
  const client = await connectPi(root, () => {}, () => {}, {
    listModels: () => [model],
    async setModel(id) {
      expect(id).toBe(model.id);
      await client.request("event.report", { event: { type: "session.model_updated", data: { model: id } } });
    },
    async onReconnect() { await client.request("event.report", {}); },
  });
  onTestFinished(() => client.close());
  client.registered({ sessionId: "s", runtimeInstanceId: "r", clientSessionKey: "native" });
  await client.request("begin", {});
  await vi.waitFor(() => expect(responses).toHaveLength(2));
  expect(responses).toContainEqual({ jsonrpc: "2.0", id: "list", result: { models: [model] } });
  expect(responses).toContainEqual({ jsonrpc: "2.0", id: "set", result: { accepted: true } });
  await expect(client.request("break", {})).rejects.toThrow();
  await vi.waitFor(() => expect(methods.slice(-2)).toEqual(["runtime.attach", "event.report"]));
});


test("model control preserves uncertain outcomes across the RPC boundary", async () => {
  let result: any;
  const root = await server((socket, message) => {
    if (!message.method) { result = message; return; }
    socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: "set", method: "model.set", params: { model: "provider/model" } })}\n`);
    reply(socket, message.id, {});
  });
  const client = await connectPi(root, () => {}, () => {}, {
    listModels: () => [],
    async setModel() { throw new RpcError(-32007, "Observation acknowledgement lost"); },
    async onReconnect() {},
  });
  onTestFinished(() => client.close());
  await client.request("begin", {});
  await vi.waitFor(() => expect(result?.error?.code).toBe(-32007));
});
