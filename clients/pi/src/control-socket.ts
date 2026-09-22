import { mkdtemp, rmdir, stat } from "node:fs/promises";
import { createServer, type Socket } from "node:net";
import { isAbsolute, join } from "node:path";

export const CONTROL_VERSION = 1;
export const MAX_CONTROL_FRAME_BYTES = 64 * 1024;
const HANDSHAKE_TIMEOUT_MS = 5_000;

export interface ControlIdentity {
  sessionId: string;
  runtimeInstanceId: string;
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
      socket.end(`${JSON.stringify({ version: CONTROL_VERSION, request_id: null, error: {
        code: "connection_busy", message: "Pi control endpoint already has a controller",
      } })}\n`);
      socket.destroySoon();
      return;
    }
    active = socket;
    let greeted = false;
    let failed = false;
    let pending = Buffer.alloc(0);

    const respond = (message: object) => {
      if (!socket.write(`${JSON.stringify({ version: CONTROL_VERSION, ...message })}\n`)) {
        socket.destroy(new Error("Pi control response backpressure limit reached"));
      }
    };
    const fail = (code: string, message: string, requestId: string | null = null) => {
      failed = true;
      onError(new Error(`${code}: ${message}`));
      respond({ request_id: requestId, error: { code, message } });
      socket.destroySoon();
    };
    const timer = setTimeout(() => fail("handshake_timeout", "Pi control handshake timed out"), HANDSHAKE_TIMEOUT_MS);
    timer.unref();
    socket.on("close", () => {
      clearTimeout(timer);
      if (active === socket) active = undefined;
    });

    const handle = (frame: Buffer) => {
      let request: Record<string, unknown>;
      try {
        const value: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(frame));
        if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("expected object");
        request = value as Record<string, unknown>;
      } catch {
        fail("invalid_message", "Expected a UTF-8 JSON object");
        return;
      }
      const requestId = request.request_id;
      if (typeof requestId !== "string" || !requestId || requestId.length > 128) {
        fail("invalid_message", "request_id must contain 1 to 128 characters");
        return;
      }
      if (request.version !== CONTROL_VERSION) {
        fail("unsupported_version", "Unsupported Pi control protocol version", requestId);
      } else if (!greeted) {
        if (request.method !== "hello") {
          fail("handshake_required", "The first request must be hello", requestId);
        } else if (request.session_id !== identity.sessionId || request.runtime_instance_id !== identity.runtimeInstanceId) {
          fail("identity_mismatch", "Pi control runtime identity does not match", requestId);
        } else {
          greeted = true;
          clearTimeout(timer);
          respond({ request_id: requestId, result: {
            session_id: identity.sessionId, runtime_instance_id: identity.runtimeInstanceId,
          } });
        }
      } else if (request.method === "ping") {
        respond({ request_id: requestId, result: { pong: true } });
      } else {
        respond({ request_id: requestId, error: { code: "unknown_method", message: "Unknown Pi control method" } });
      }
    };

    socket.on("data", (chunk: Buffer) => {
      if (failed) return;
      let offset = 0;
      while (offset < chunk.length && !failed && !socket.destroyed) {
        const newline = chunk.indexOf(10, offset);
        const end = newline === -1 ? chunk.length : newline;
        const part = chunk.subarray(offset, end);
        if (pending.length + part.length > MAX_CONTROL_FRAME_BYTES) {
          fail("message_too_large", "Pi control frame exceeds 64 KiB");
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
