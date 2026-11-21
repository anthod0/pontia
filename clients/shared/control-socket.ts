import { RpcError } from "./rpc-error.js";
export { RpcError } from "./rpc-error.js";
import { createConnection, type Socket } from "node:net";
import { isAbsolute, join } from "node:path";
import { statSync } from "node:fs";

export const CONTROL_VERSION = 8;
export const MAX_CONTROL_FRAME_BYTES = 64 * 1024;
// Match the former HTTP event body limit, including the RPC envelope.
export const MAX_RPC_FRAME_BYTES = 2 * 1024 * 1024 + 1024;
const REQUEST_TIMEOUT_MS = 5_000;

export interface ControlIdentity {
  sessionId: string;
  runtimeId: string;
  clientSessionKey?: string;
}
export interface RuntimeConnection {
  request(method: string, params: object): Promise<unknown>;
  registered(identity: ControlIdentity): void;
  close(): Promise<void>;
}

export function controlSocketPath(pontiaHome: string): string {
  if (!isAbsolute(pontiaHome) || pontiaHome.includes("\0"))
    throw new Error("Control home requires an absolute path without NUL");
  const home = statSync(pontiaHome, { bigint: true });
  return join(
    `/tmp/pontia-control-${home.uid}-${home.dev.toString(16)}-${home.ino.toString(16)}`,
    "rpc.sock",
  );
}

class RpcSocket {
  private sequence = 0;
  private pending = new Map<
    string,
    {
      resolve(value: unknown): void;
      reject(error: Error): void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  private buffered = Buffer.alloc(0);
  private failed = false;

  private socket: Socket;
  private onRequest: ControlHandler;
  private keepAlive: boolean;

  constructor(socket: Socket, onRequest: ControlHandler, keepAlive: boolean) {
    this.socket = socket;
    this.onRequest = onRequest;
    this.keepAlive = keepAlive;
    socket.on("data", (chunk: Buffer) => this.receive(chunk));
    socket.on("error", (error) => this.fail(error));
    socket.on("close", () =>
      this.fail(new Error("Control RPC connection closed; requests were not replayed")),
    );
  }

  close(): void {
    this.fail(new Error("Control RPC connection closed"));
  }

  request(method: string, params: object): Promise<unknown> {
    if (this.failed) return Promise.reject(new Error("Control RPC connection is closed"));
    const id = `runtime:${this.sequence++}`;
    const encoded = this.encode({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => this.fail(new Error("Control RPC timed out; request was not replayed")),
        REQUEST_TIMEOUT_MS,
      );
      timer.unref();
      // RPC mode installs its stdin listener only after extension registration.
      this.socket.ref();
      this.pending.set(id, { resolve, reject, timer });
      this.write(encoded);
    });
  }

  private encode(value: object): string {
    const encoded = JSON.stringify(value);
    if (Buffer.byteLength(encoded) > MAX_RPC_FRAME_BYTES)
      throw new RpcError(-32602, "Control RPC frame exceeds size limit");
    return `${encoded}\n`;
  }

  private write(encoded: string, afterWrite?: () => void): void {
    if (this.socket.writableLength + Buffer.byteLength(encoded) > 2 * MAX_RPC_FRAME_BYTES) {
      this.fail(new Error("Control RPC backpressure limit reached"));
      return;
    }
    this.socket.write(encoded, afterWrite);
  }

  private fail(error: Error): void {
    if (this.failed) return;
    this.failed = true;
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    this.socket.destroy();
  }

  private receive(chunk: Buffer): void {
    let offset = 0;
    try {
      while (offset < chunk.length && !this.failed) {
        const newline = chunk.indexOf(10, offset);
        const end = newline === -1 ? chunk.length : newline;
        const part = chunk.subarray(offset, end);
        if (this.buffered.length + part.length > MAX_RPC_FRAME_BYTES)
          throw new Error("Control RPC frame exceeds size limit");
        this.buffered = Buffer.concat([this.buffered, part]);
        if (newline === -1) return;
        const frame = this.buffered;
        this.buffered = Buffer.alloc(0);
        this.handle(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(frame)));
        offset = newline + 1;
      }
    } catch (error) {
      this.fail(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private handle(value: unknown): void {
    if (!value || typeof value !== "object" || Array.isArray(value))
      throw new Error("Invalid Control RPC frame");
    const message = value as Record<string, any>;
    if (message.jsonrpc !== "2.0") throw new Error("Invalid Control RPC version");
    if (Object.hasOwn(message, "method")) {
      if (
        typeof message.method !== "string" ||
        Object.hasOwn(message, "result") ||
        Object.hasOwn(message, "error") ||
        (message.id !== undefined &&
          typeof message.id !== "string" &&
          typeof message.id !== "number")
      ) {
        throw new Error("Invalid Control RPC request");
      }
      const respond = (body: object, afterWrite?: () => void) => {
        if (message.id !== undefined)
          this.write(this.encode({ jsonrpc: "2.0", id: message.id, ...body }), afterWrite);
        else afterWrite?.();
      };
      const error = (code: number, message: string) => respond({ error: { code, message } });
      const params = message.params === undefined ? {} : message.params;
      if (!params || typeof params !== "object" || Array.isArray(params)) {
        error(-32602, "Expected named parameters");
        return;
      }
      this.onRequest(
        message.method,
        params,
        respond,
        error,
        Buffer.byteLength(JSON.stringify(message)),
      );

      return;
    }
    if (
      Object.hasOwn(message, "result") === Object.hasOwn(message, "error") ||
      (message.error !== undefined &&
        (!message.error ||
          !Number.isInteger(message.error.code) ||
          typeof message.error.message !== "string"))
    ) {
      throw new Error("Invalid Control RPC response");
    }
    const pending = this.pending.get(message.id);
    if (!pending) throw new Error("Unexpected Control RPC response ID");
    this.pending.delete(message.id);
    if (!this.pending.size && !this.keepAlive) this.socket.unref();
    clearTimeout(pending.timer);
    if (message.error) pending.reject(new RpcError(message.error.code, message.error.message));
    else pending.resolve(message.result);
  }
}

export type ControlHandler = (
  method: string,
  params: Record<string, any>,
  respond: (body: object, afterWrite?: () => void) => void,
  error: (code: number, message: string) => void,
  frameBytes: number,
) => void;

export async function connectRuntime(
  pontiaHome: string,
  onError: (error: Error) => void,
  onRequest: ControlHandler,
  onReconnect?: () => Promise<void>,
  onDisconnect?: () => void,
  keepAlive = false,
): Promise<RuntimeConnection> {
  const path = controlSocketPath(pontiaHome);
  let identity: ControlIdentity | undefined;
  let stopped = false;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let current: RpcSocket | undefined;
  const connecting = new Set<Socket>();
  const peers = new Set<RpcSocket>();
  let reconnectRejected = false;
  let attached = false;
  let delay = 250;
  const waiting = new Set<() => void>();

  async function ready(): Promise<void> {
    if (!identity || attached || stopped || reconnectRejected) return;
    await new Promise<void>((resolve, reject) => {
      const wake = () => {
        clearTimeout(timer);
        waiting.delete(wake);
        resolve();
      };
      const timer = setTimeout(() => {
        waiting.delete(wake);
        reject(new Error("Control reconnect timed out"));
      }, REQUEST_TIMEOUT_MS);
      waiting.add(wake);
    });
  }

  function wakeRequests(): void {
    for (const wake of waiting) wake();
  }

  async function open(): Promise<RpcSocket> {
    const socket = createConnection(path);
    connecting.add(socket);
    if (!keepAlive) socket.unref();
    try {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(
          () => socket.destroy(new Error("Control socket connection timed out")),
          REQUEST_TIMEOUT_MS,
        );
        socket.once("error", reject);
        socket.once("connect", () => {
          clearTimeout(timer);
          socket.off("error", reject);
          resolve();
        });
        socket.once("close", () => {
          clearTimeout(timer);
          reject(new Error("Control socket closed during connect"));
        });
      });
    } finally {
      connecting.delete(socket);
    }
    if (stopped) {
      socket.destroy();
      throw new Error("Control connection is closed");
    }
    const peer = new RpcSocket(socket, onRequest, keepAlive);
    peers.add(peer);
    socket.once("close", () => {
      peers.delete(peer);
      if (current === peer) {
        current = undefined;
        attached = false;
        onDisconnect?.();
        reconnect();
      }
    });
    return peer;
  }

  function reconnect(): void {
    if (stopped || reconnectRejected || !identity || retry) return;
    retry = setTimeout(async () => {
      retry = undefined;
      try {
        const peer = await open();
        current = peer;
        const result = (await peer.request("runtime.attach", {
          version: CONTROL_VERSION,
          session_id: identity!.sessionId,
          runtime_id: identity!.runtimeId,
          client_session_key: identity!.clientSessionKey,
        })) as Record<string, unknown>;
        if (
          result?.session_id !== identity!.sessionId ||
          result?.runtime_id !== identity!.runtimeId
        ) {
          throw new RpcError(-32009, "Control reconnect identity mismatch");
        }
        attached = true;
        delay = 250;
        wakeRequests();
        await onReconnect?.();
      } catch (error) {
        onError(error instanceof Error ? error : new Error(String(error)));
        // A rejected identity cannot be repaired by repeating registration.
        if (error instanceof RpcError && error.code !== -32603) {
          reconnectRejected = true;
          wakeRequests();
        }
        current?.close();
        current = undefined;
        delay = Math.min(delay * 2, 5000);
        reconnect();
      }
    }, delay);
    if (!keepAlive) retry.unref();
  }

  current = await open();
  return {
    async request(method, params) {
      await ready();
      if (!current || stopped)
        return Promise.reject(new Error("Control connection is unavailable"));
      return current.request(method, params);
    },
    registered(value) {
      identity = value;
      attached = !!current;
      if (!current) reconnect();
    },
    async close() {
      stopped = true;
      wakeRequests();
      if (retry) clearTimeout(retry);
      for (const socket of connecting) socket.destroy();
      for (const peer of peers) peer.close();
      current = undefined;
    },
  };
}
