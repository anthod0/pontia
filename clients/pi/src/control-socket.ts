import { mkdtemp, rmdir, stat } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { isAbsolute, join } from "node:path";

export const CONTROL_VERSION = 2;
export const MAX_CONTROL_FRAME_BYTES = 64 * 1024;
const HANDSHAKE_TIMEOUT_MS = 5_000;

type RpcId = number | string | null;
const ERROR_CODES = {
  parse_error: -32700,
  invalid_request: -32600,
  unknown_method: -32601,
  invalid_params: -32602,
  connection_busy: -32001,
  handshake_required: -32002,
  identity_mismatch: -32003,
  handshake_timeout: -32004,
  message_too_large: -32005,
  submit_rejected: -32006,
} as const;
type ErrorKind = keyof typeof ERROR_CODES;

function rpcError(id: RpcId, kind: ErrorKind, message: string) {
  return { jsonrpc: "2.0", id, error: { code: ERROR_CODES[kind], message, data: { code: kind } } };
}

export interface ControlIdentity {
  sessionId: string;
  runtimeInstanceId: string;
}

export interface ControlInput {
  input: string;
  inboxMessageId?: string;
}

export interface ControlSocket {
  socketPath: string;
  close(): Promise<void>;
}

export function controlSocketDirectory(env: Record<string, string | undefined>): string {
  return env.XDG_RUNTIME_DIR || "/tmp";
}

export function validateControlSocketPath(path: string): void {
  if (!isAbsolute(path) || path.includes("\0") || Buffer.byteLength(path) > 103) {
    throw new Error("Pi control socket requires an absolute path of at most 103 bytes without NUL");
  }
}

export async function startControlSocket(
  identity: ControlIdentity,
  env: Record<string, string | undefined> = process.env,
  onError: (error: Error) => void = () => {},
  onSubmit?: (input: ControlInput) => void,
): Promise<ControlSocket> {
  const base = controlSocketDirectory(env);
  validateControlSocketPath(join(base, "pontia-pi-XXXXXX", "control.sock"));
  if (env.XDG_RUNTIME_DIR) {
    const info = await stat(base);
    if (!info.isDirectory() || info.uid !== process.getuid?.() || (info.mode & 0o777) !== 0o700) {
      throw new Error("XDG_RUNTIME_DIR must be a directory owned by the current user with mode 0700");
    }
  }
  const directory = await mkdtemp(join(base, "pontia-pi-"));
  const socketPath = join(directory, "control.sock");
  let active: Socket | undefined;
  let closing = false;
  let closePromise: Promise<void> | undefined;

  const server = createServer((socket) => {
    socket.on("error", onError);
    if (closing || active) {
      socket.end(`${JSON.stringify(rpcError(null, "connection_busy", "Pi control endpoint already has a controller"))}\n`);
      socket.destroySoon();
      return;
    }
    active = socket;
    let greeted = false;
    let failed = false;
    let pending = Buffer.alloc(0);

    const respond = (message: object) => {
      const encoded = JSON.stringify(message);
      if (Buffer.byteLength(encoded) > MAX_CONTROL_FRAME_BYTES) {
        failed = true;
        socket.destroy(new Error("Pi control response exceeds 64 KiB"));
        return;
      }
      if (!socket.write(`${encoded}\n`)) {
        socket.destroy(new Error("Pi control response backpressure limit reached"));
      }
    };
    const fail = (kind: ErrorKind, message: string, id: RpcId = null) => {
      failed = true;
      onError(new Error(`${kind}: ${message}`));
      return rpcError(id, kind, message);
    };
    const timer = setTimeout(() => {
      respond(fail("handshake_timeout", "Pi control handshake timed out"));
      socket.destroySoon();
    }, HANDSHAKE_TIMEOUT_MS);
    timer.unref();
    socket.on("close", () => {
      clearTimeout(timer);
      if (active === socket) active = undefined;
    });

    const handleRequest = (value: unknown): object | undefined => {
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        return rpcError(null, "invalid_request", "Expected a JSON-RPC request object");
      }
      const request = value as Record<string, unknown>;
      const hasId = Object.hasOwn(request, "id");
      if (request.jsonrpc !== "2.0" || typeof request.method !== "string"
        || (hasId && request.id !== null && typeof request.id !== "string" && typeof request.id !== "number")
        || (typeof request.id === "number" && !Number.isFinite(request.id))) {
        return rpcError(null, "invalid_request", "Invalid JSON-RPC 2.0 request");
      }
      const id = hasId ? request.id as RpcId : null;
      const reply = (response: object) => hasId ? response : undefined;
      const result = (data: object) => reply({ jsonrpc: "2.0", id, result: data });
      const error = (kind: ErrorKind, message: string) => reply(rpcError(id, kind, message));
      if (failed) return error("handshake_required", "Pi control handshake failed");
      if (request.params !== undefined && (!request.params || typeof request.params !== "object" || Array.isArray(request.params))) {
        return error("invalid_params", "Pi control methods require named parameters");
      }
      const params = (request.params ?? {}) as Record<string, unknown>;
      if (!greeted) {
        if (request.method !== "hello") {
          return reply(fail("handshake_required", "The first request must be hello", id));
        }
        if (typeof params.session_id !== "string" || typeof params.runtime_instance_id !== "string") {
          return error("invalid_params", "hello requires session_id and runtime_instance_id");
        }
        if (params.session_id !== identity.sessionId || params.runtime_instance_id !== identity.runtimeInstanceId) {
          return reply(fail("identity_mismatch", "Pi control runtime identity does not match", id));
        }
        greeted = true;
        clearTimeout(timer);
        return result({ session_id: identity.sessionId, runtime_instance_id: identity.runtimeInstanceId });
      } else if (request.method === "ping") {
        return result({ pong: true });
      } else if (request.method === "submit") {
        if (typeof params.input !== "string" || !params.input.trim()
          || (params.inbox_message_id != null && (typeof params.inbox_message_id !== "string" || !params.inbox_message_id))) {
          return error("invalid_params", "submit requires non-empty input and an optional inbox_message_id");
        }
        try {
          if (!onSubmit) throw new Error("Pi input delivery is unavailable");
          onSubmit({ input: params.input, inboxMessageId: params.inbox_message_id as string | undefined });
          return result({ accepted: true });
        } catch (error) {
          return reply(rpcError(id, "submit_rejected", error instanceof Error ? error.message : String(error)));
        }
      } else {
        return error("unknown_method", "Unknown Pi control method");
      }
    };

    const handle = (frame: Buffer) => {
      let value: unknown;
      try {
        value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(frame));
      } catch {
        respond(fail("parse_error", "Expected valid UTF-8 JSON"));
        socket.destroySoon();
        return;
      }
      if (Array.isArray(value) && value.length > 0) {
        const responses = [];
        for (const request of value) {
          const response = handleRequest(request);
          if (response) responses.push(response);
        }
        if (responses.length > 0) respond(responses);
      } else {
        const response = handleRequest(value);
        if (response) respond(response);
      }
      if (failed) socket.destroySoon();
    };

    socket.on("data", (chunk: Buffer) => {
      if (failed) return;
      let offset = 0;
      while (offset < chunk.length && !failed && !socket.destroyed) {
        const newline = chunk.indexOf(10, offset);
        const end = newline === -1 ? chunk.length : newline;
        const part = chunk.subarray(offset, end);
        if (pending.length + part.length > MAX_CONTROL_FRAME_BYTES) {
          respond(fail("message_too_large", "Pi control frame exceeds 64 KiB"));
          socket.destroySoon();
          return;
        }
        pending = Buffer.concat([pending, part]);
        if (newline === -1) return;
        handle(pending);
        pending = Buffer.alloc(0);
        offset = newline + 1;
      }
    });
  });
  server.on("error", onError);
  try {
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(socketPath, () => {
        server.off("error", reject);
        resolve();
      });
    });
  } catch (error) {
    await rmdir(directory);
    throw error;
  }

  return {
    socketPath,
    close() {
      closePromise ??= (async () => {
        closing = true;
        const closed = new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
        active?.destroy();
        await closed;
        await rmdir(directory);
      })();
      return closePromise;
    },
  };
}
