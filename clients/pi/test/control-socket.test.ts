import { mkdir } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { join } from "node:path";
import { onTestFinished, expect, test, vi } from "vitest";
import { connectPi, piSocketPath, MAX_CONTROL_FRAME_BYTES } from "../src/control-socket.js";
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
      : kind === "oversized" ? "x".repeat(MAX_CONTROL_FRAME_BYTES + 1) : "not-json";
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
