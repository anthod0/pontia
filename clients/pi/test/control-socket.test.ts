import { once } from "node:events";
import { chmod, readdir, stat } from "node:fs/promises";
import { createConnection, type Socket } from "node:net";
import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { describe, expect, onTestFinished, test, vi } from "vitest";
import { controlSocketDirectory, MAX_CONTROL_FRAME_BYTES, startControlSocket, validateControlSocketPath } from "../src/control-socket.js";
import { tempDir } from "./temp-dir.js";

const identity = { sessionId: "sess_pi", runtimeInstanceId: "rtinst_pi" };
const hello = { version: 1, request_id: "hello", method: "hello", session_id: identity.sessionId, runtime_instance_id: identity.runtimeInstanceId };
const ping = { version: 1, request_id: "ping", method: "ping" };

async function endpoint() {
  const root = await tempDir("pc-");
  const server = await startControlSocket(identity, { XDG_RUNTIME_DIR: root });
  onTestFinished(() => server.close());
  return { server, root };
}

async function connect(path: string) {
  const socket = createConnection(path);
  socket.on("error", () => {});
  const lines = createInterface({ input: socket })[Symbol.asyncIterator]();
  onTestFinished(() => { socket.destroy(); });
  await once(socket, "connect");
  return {
    socket,
    async read() {
      const line = await lines.next();
      if (line.done) throw new Error("connection closed");
      return JSON.parse(line.value);
    },
    send(message: object) { socket.write(`${JSON.stringify(message)}\n`); },
  };
}

async function disconnect(socket: Socket) {
  const closed = once(socket, "close");
  socket.end();
  await closed;
}

describe("Pi control socket", () => {
  test("selects XDG_RUNTIME_DIR, falling back to /tmp only when unset or empty", () => {
    expect(controlSocketDirectory({ XDG_RUNTIME_DIR: "/run/user/1000" })).toBe("/run/user/1000");
    expect(controlSocketDirectory({})).toBe("/tmp");
    expect(controlSocketDirectory({ XDG_RUNTIME_DIR: "" })).toBe("/tmp");
  });

  test("frames split and coalesced requests and keeps the first connection when rejecting another", async () => {
    const { server } = await endpoint();
    const first = await connect(server.socketPath);
    const encoded = JSON.stringify(hello);
    first.socket.write(encoded.slice(0, 13));
    first.socket.write(`${encoded.slice(13)}\n${JSON.stringify(ping)}\n`);
    expect(await first.read()).toMatchObject({ request_id: "hello", result: { runtime_instance_id: identity.runtimeInstanceId } });
    expect(await first.read()).toMatchObject({ request_id: "ping", result: { pong: true } });

    const second = await connect(server.socketPath);
    expect(await second.read()).toMatchObject({ error: { code: "connection_busy" } });
    first.send({ ...ping, request_id: "still-alive" });
    expect(await first.read()).toMatchObject({ request_id: "still-alive", result: { pong: true } });
    await disconnect(first.socket);
    const replacement = await connect(server.socketPath);
    replacement.send(hello);
    expect(await replacement.read()).toHaveProperty("result");
    replacement.send(ping);
    expect(await replacement.read()).toMatchObject({ result: { pong: true } });
  });

  test.each([
    [{ ...hello, runtime_instance_id: "rtinst_stale" }, "identity_mismatch"],
    [{ ...hello, session_id: "sess_other" }, "identity_mismatch"],
    [{ ...hello, version: 2 }, "unsupported_version"],
    [ping, "handshake_required"],
  ])("rejects invalid handshake %j", async (request, code) => {
    const { server } = await endpoint();
    const client = await connect(server.socketPath);
    client.send(request);
    expect(await client.read()).toMatchObject({ request_id: request.request_id, error: { code } });
    await expect(client.read()).rejects.toThrow("connection closed");
  });

  test("rejects malformed and oversized frames and releases the connection", async () => {
    const { server } = await endpoint();
    const malformed = await connect(server.socketPath);
    malformed.socket.write("[invalid\n");
    expect(await malformed.read()).toMatchObject({ error: { code: "invalid_message" } });
    await expect(malformed.read()).rejects.toThrow("connection closed");
    const oversized = await connect(server.socketPath);
    oversized.socket.write(Buffer.alloc(MAX_CONTROL_FRAME_BYTES + 1, 65));
    expect(await oversized.read()).toMatchObject({ error: { code: "message_too_large" } });
  });

  test("unknown methods fail explicitly without breaking the channel", async () => {
    const { server } = await endpoint();
    const client = await connect(server.socketPath);
    client.send(hello);
    await client.read();
    client.send({ ...ping, method: "unknown" });
    expect(await client.read()).toMatchObject({ request_id: "ping", error: { code: "unknown_method" } });
    client.send(ping);
    expect(await client.read()).toMatchObject({ result: { pong: true } });
  });

  test("a connection that never handshakes times out and frees the endpoint", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    onTestFinished(() => { vi.useRealTimers(); });
    const { server } = await endpoint();
    const client = await connect(server.socketPath);
    await vi.advanceTimersByTimeAsync(5_001);
    expect(await client.read()).toMatchObject({ error: { code: "handshake_timeout" } });
    await expect(client.read()).rejects.toThrow("connection closed");
    const next = await connect(server.socketPath);
    next.send(hello);
    expect(await next.read()).toHaveProperty("result");
  });

  test("uses a private directory, closes clients, and removes only its own endpoint", async () => {
    const { server, root } = await endpoint();
    const other = await startControlSocket(identity, { XDG_RUNTIME_DIR: root });
    onTestFinished(() => other.close());
    expect((await stat(dirname(server.socketPath))).mode & 0o777).toBe(0o700);
    const client = await connect(server.socketPath);
    const closed = once(client.socket, "close");
    await server.close();
    await closed;
    expect(await readdir(root)).toHaveLength(1);
    expect((await stat(other.socketPath)).isSocket()).toBe(true);
  });

  test("checks path length in bytes and refuses unsafe configured directories", async () => {
    expect(() => validateControlSocketPath("/" + "a".repeat(102))).not.toThrow();
    expect(() => validateControlSocketPath("/" + "a".repeat(103))).toThrow("103 bytes");
    expect(() => validateControlSocketPath("/" + "界".repeat(35))).toThrow("103 bytes");
    const root = await tempDir("pc-");
    await chmod(root, 0o755);
    await expect(startControlSocket(identity, { XDG_RUNTIME_DIR: root })).rejects.toThrow("0700");
    expect(await readdir(root)).toEqual([]);
  });
});
