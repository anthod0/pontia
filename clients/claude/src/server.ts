import { createHash, randomUUID, timingSafeEqual } from "node:crypto";
import { chmod, lstat } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { dirname } from "node:path";
import { Host, ControlError, type QueryFactory } from "./host.js";
import type { HostEvent } from "./facts.js";

const MAX_FRAME = 1024 * 1024;
const RECEIPTS = 128;
export interface Bootstrap {
  session_id: string;
  runtime_id: string;
  socket_path: string;
  token: string;
  cwd: string;
  resume?: string;
}

type ObjectValue = Record<string, unknown>;
function object(value: unknown): ObjectValue {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new ControlError("rejected", "invalid_request");
  return value as ObjectValue;
}
function string(value: unknown): string {
  if (typeof value !== "string" || !value) throw new ControlError("rejected", "invalid_request");
  return value;
}

export function bootstrap(value: unknown): Bootstrap {
  const data = object(value);
  const result: Bootstrap = {
    session_id: string(data.session_id),
    runtime_id: string(data.runtime_id),
    socket_path: string(data.socket_path),
    token: string(data.token),
    cwd: string(data.cwd),
  };
  if (data.resume !== undefined) result.resume = string(data.resume);
  if (!/^[0-9a-f]{64}$/.test(result.token)) throw new ControlError("rejected", "invalid_token");
  return result;
}

export async function serve(config: Bootstrap, factory?: QueryFactory) {
  const directory = await lstat(dirname(config.socket_path));
  if (
    !directory.isDirectory() ||
    directory.isSymbolicLink() ||
    (directory.mode & 0o077) !== 0 ||
    directory.uid !== process.getuid?.()
  )
    throw new Error("socket_directory_not_private");
  const generation = randomUUID();
  let current: Socket | undefined;
  let sequence = 0;
  let highWater = 0;
  let stopped = false;
  const sockets = new Set<Socket>();
  const receipts = new Map<number, { hash: string; response: Promise<ObjectValue> }>();
  const send = (socket: Socket, data: unknown) => {
    const frame = JSON.stringify(data) + "\n";
    if (
      Buffer.byteLength(frame) > MAX_FRAME ||
      socket.writableLength + Buffer.byteLength(frame) > MAX_FRAME
    ) {
      socket.destroy();
      if (current === socket) {
        current = undefined;
        host.controlLost();
      }
      return;
    }
    if (!socket.destroyed) socket.write(frame);
  };
  const host = new Host(
    config,
    (event: HostEvent) => {
      sequence += 1;
      if (current) send(current, { type: "event", host_id: generation, sequence, event });
    },
    factory,
  );

  const invoke = async (request: ObjectValue): Promise<ObjectValue> => {
    try {
      switch (request.method) {
        case "input":
          host.submit(string(request.input_id), string(request.text));
          break;
        case "interrupt":
          await host.interrupt(string(request.input_id));
          break;
        case "permission":
          host.resolvePermission(string(request.permission_id), string(request.option));
          break;
        case "close":
          await host.close();
          break;
        default:
          throw new ControlError("unsupported", "unsupported_method");
      }
      return { status: "accepted" };
    } catch (error) {
      return error instanceof ControlError
        ? { status: error.code, reason: error.reason }
        : { status: "unknown", reason: "operation_unconfirmed" };
    }
  };

  const server = createServer((socket) => {
    sockets.add(socket);
    let authenticated = false;
    let buffer = Buffer.alloc(0);
    socket.on("error", () => {});
    socket.on("close", () => {
      sockets.delete(socket);
      if (current === socket) {
        current = undefined;
        if (!stopped && !host.status().closed) host.controlLost();
      }
    });
    socket.on("data", (chunk) => {
      buffer = Buffer.concat([buffer, Buffer.from(chunk)]);
      for (;;) {
        const newline = buffer.indexOf(10);
        if (newline < 0) {
          if (buffer.length > MAX_FRAME) socket.destroy();
          return;
        }
        if (newline + 1 > MAX_FRAME) {
          socket.destroy();
          return;
        }
        const line = buffer.subarray(0, newline).toString("utf8");
        buffer = buffer.subarray(newline + 1);
        try {
          const request = object(JSON.parse(line));
          if (!authenticated) {
            const token = Buffer.from(typeof request.token === "string" ? request.token : "");
            const expected = Buffer.from(config.token);
            if (
              request.method !== "attach" ||
              request.session_id !== config.session_id ||
              request.runtime_id !== config.runtime_id ||
              token.length !== expected.length ||
              !timingSafeEqual(token, expected)
            ) {
              socket.destroy();
              return;
            }
            if (current) {
              // A replacement does not inherit pending replies or replay missed facts.
              current.destroy();
              current = undefined;
              host.controlLost();
            }
            current = socket;
            host.controlRestored(
              request.last_host_id === generation && request.last_sequence === sequence,
            );
            authenticated = true;
            send(socket, {
              type: "attached",
              host_id: generation,
              sequence,
              high_water: highWater,
              ...host.status(),
            });
            continue;
          }
          if (current !== socket || request.host_id !== generation) {
            socket.destroy();
            return;
          }
          const id = request.id;
          if (typeof id !== "number" || !Number.isSafeInteger(id) || id < 1)
            throw new ControlError("rejected", "invalid_request_id");
          const hash = createHash("sha256").update(line).digest("hex");
          let receipt = receipts.get(id);
          if (receipt) {
            if (receipt.hash !== hash) throw new ControlError("rejected", "request_id_reused");
          } else {
            if (id <= highWater) throw new ControlError("rejected", "request_expired");
            highWater = id;
            receipt = { hash, response: invoke(request) };
            receipts.set(id, receipt);
            if (receipts.size > RECEIPTS) receipts.delete(receipts.keys().next().value!);
          }
          void receipt.response.then((response) => {
            if (current === socket && !socket.destroyed) {
              send(socket, { type: "response", host_id: generation, id, ...response });
            }
            if (request.method === "close" && response.status === "accepted" && !stopped) {
              stopped = true;
              server.close();
              for (const peer of sockets) peer.destroySoon();
            }
          });
        } catch (error) {
          // Malformed or ambiguous requests are never executed, nor echoed into diagnostics.
          send(socket, {
            type: "error",
            status: "rejected",
            reason: error instanceof ControlError ? error.reason : "invalid_request",
          });
        }
      }
    });
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(config.socket_path, () => {
      server.off("error", reject);
      resolve();
    });
  });
  // Parent directory already excludes other users during the bind/chmod window.
  try {
    await chmod(config.socket_path, 0o600);
  } catch (error) {
    server.close();
    throw error;
  }
  return {
    async close() {
      stopped = true;
      await host.close();
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };
}
