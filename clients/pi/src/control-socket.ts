import { createConnection, type Socket } from "node:net";
import { isAbsolute, join } from "node:path";

export const CONTROL_VERSION = 6;
export const MAX_CONTROL_FRAME_BYTES = 64 * 1024;
// Match the former HTTP event body limit, including the RPC envelope.
export const MAX_RPC_FRAME_BYTES = 2 * 1024 * 1024 + 1024;
const REQUEST_TIMEOUT_MS = 5_000;

export interface ControlIdentity {
  sessionId: string;
  runtimeInstanceId: string;
  clientSessionKey?: string;
}
export interface ControlInput { input: string; inboxMessageId?: string }
export interface PiModel { id: string; name: string; description: string }
export interface ModelControl {
  listModels(): PiModel[];
  setModel(model: string): Promise<void>;
  onReconnect(): Promise<void>;
}
export interface LifecycleControl {
  interrupt(): void;
  shutdown(): void;
}
export interface PiConnection {
  request(method: string, params: object): Promise<unknown>;
  registered(identity: ControlIdentity): void;
  close(): Promise<void>;
}
export class RpcError extends Error {
  readonly code: number;
  constructor(code: number, message: string) { super(message); this.code = code; }
}

export function piSocketPath(pontiaHome: string): string {
  const path = join(pontiaHome, "state", "pi", "rpc.sock");
  if (!isAbsolute(path) || path.includes("\0") || Buffer.byteLength(path) > 103) {
    throw new Error("Pi Unix socket requires an absolute path of at most 103 bytes without NUL");
  }
  return path;
}

class RpcSocket {
  private sequence = 0;
  private pending = new Map<string, { resolve(value: unknown): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }>();
  private buffered = Buffer.alloc(0);
  private failed = false;

  private socket: Socket;
  private onSubmit: (input: ControlInput) => void;
  private models?: ModelControl;
  private lifecycle?: LifecycleControl;
  private onReplay?: (inboxMessageId: string) => void;

  constructor(socket: Socket, onSubmit: (input: ControlInput) => void, models?: ModelControl, onReplay?: (inboxMessageId: string) => void, lifecycle?: LifecycleControl) {
    this.socket = socket;
    this.onSubmit = onSubmit;
    this.models = models;
    this.onReplay = onReplay;
    this.lifecycle = lifecycle;
    socket.on("data", (chunk: Buffer) => this.receive(chunk));
    socket.on("error", (error) => this.fail(error));
    socket.on("close", () => this.fail(new Error("Pi RPC connection closed; requests were not replayed")));
  }

  close(): void { this.fail(new Error("Pi RPC connection closed")); }

  request(method: string, params: object): Promise<unknown> {
    if (this.failed) return Promise.reject(new Error("Pi RPC connection is closed"));
    const id = `pi:${this.sequence++}`;
    const encoded = this.encode({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.fail(new Error("Pi RPC timed out; request was not replayed")), REQUEST_TIMEOUT_MS);
      timer.unref();
      this.pending.set(id, { resolve, reject, timer });
      this.write(encoded);
    });
  }

  private encode(value: object): string {
    const encoded = JSON.stringify(value);
    if (Buffer.byteLength(encoded) > MAX_RPC_FRAME_BYTES) throw new RpcError(-32602, "Pi RPC frame exceeds size limit");
    return `${encoded}\n`;
  }

  private write(encoded: string): void {
    if (this.socket.writableLength + Buffer.byteLength(encoded) > 2 * MAX_RPC_FRAME_BYTES) {
      this.fail(new Error("Pi RPC backpressure limit reached"));
      return;
    }
    this.socket.write(encoded);
  }

  private fail(error: Error): void {
    if (this.failed) return;
    this.failed = true;
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
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
        if (this.buffered.length + part.length > MAX_RPC_FRAME_BYTES) throw new Error("Pi RPC frame exceeds size limit");
        this.buffered = Buffer.concat([this.buffered, part]);
        if (newline === -1) return;
        const frame = this.buffered;
        this.buffered = Buffer.alloc(0);
        this.handle(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(frame)));
        offset = newline + 1;
      }
    } catch (error) { this.fail(error instanceof Error ? error : new Error(String(error))); }
  }

  private handle(value: unknown): void {
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid Pi RPC frame");
    const message = value as Record<string, any>;
    if (message.jsonrpc !== "2.0") throw new Error("Invalid Pi RPC version");
    if (Object.hasOwn(message, "method")) {
      if (typeof message.method !== "string" || Object.hasOwn(message, "result") || Object.hasOwn(message, "error")
        || (message.id !== undefined && typeof message.id !== "string" && typeof message.id !== "number")) {
        throw new Error("Invalid Pi RPC request");
      }
      const respond = (body: object) => {
        if (message.id !== undefined) this.write(this.encode({ jsonrpc: "2.0", id: message.id, ...body }));
      };
      const error = (code: number, message: string) => respond({ error: { code, message } });
      const params = message.params === undefined ? {} : message.params;
      if (!params || typeof params !== "object" || Array.isArray(params)) { error(-32602, "Expected named parameters"); return; }
      if (message.method === "ping") { respond({ result: { pong: true } }); return; }
      if (this.lifecycle && (message.method === "interrupt" || message.method === "shutdown")) {
        try {
          this.lifecycle[message.method as "interrupt" | "shutdown"]();
          respond({ result: { accepted: true } });
        } catch (failure) {
          error(failure instanceof RpcError ? failure.code : -32006, failure instanceof Error ? failure.message : String(failure));
        }
        return;
      }
      if (message.method === "branch.replay" && this.onReplay) {
        if (typeof params.inbox_message_id !== "string" || !/^msg_[^\s]+$/.test(params.inbox_message_id)) {
          error(-32602, "branch.replay requires an Inbox Message identifier"); return;
        }
        try {
          this.onReplay(params.inbox_message_id);
          respond({ result: { accepted: true } });
        } catch (failure) { error(-32006, failure instanceof Error ? failure.message : String(failure)); }
        return;
      }
      if (this.models && (message.method === "models.list" || message.method === "model.set")) {
        const models = this.models;
        if (message.method === "model.set" && (typeof params.model !== "string" || !params.model.trim())) {
          error(-32602, "model.set requires a non-empty model"); return;
        }
        // Keep reading responses while setModel emits an acknowledged model fact.
        void (async () => {
          try {
            if (message.method === "models.list") respond({ result: { models: models.listModels() } });
            else {
              await models.setModel(params.model);
              respond({ result: { accepted: true } });
            }
          } catch (failure) {
            error(failure instanceof RpcError ? failure.code : -32006, failure instanceof Error ? failure.message : String(failure));
          }
        })().catch((failure) => this.fail(failure));
        return;
      }
      if (message.method !== "submit") { error(-32601, "Unknown Pi control method"); return; }
      if (Buffer.byteLength(JSON.stringify(message)) > MAX_CONTROL_FRAME_BYTES) { error(-32602, "Pi submit frame exceeds 64 KiB"); return; }
      if (typeof params.input !== "string" || !params.input.trim()
        || (params.inbox_message_id != null && (typeof params.inbox_message_id !== "string" || !params.inbox_message_id))) {
        error(-32602, "submit requires non-empty input and optional inbox_message_id"); return;
      }
      try {
        this.onSubmit({ input: params.input, inboxMessageId: params.inbox_message_id ?? undefined });
        respond({ result: { accepted: true } });
      } catch (failure) { error(-32006, failure instanceof Error ? failure.message : String(failure)); }
      return;
    }
    if (Object.hasOwn(message, "result") === Object.hasOwn(message, "error")
      || (message.error !== undefined && (!message.error || !Number.isInteger(message.error.code) || typeof message.error.message !== "string"))) {
      throw new Error("Invalid Pi RPC response");
    }
    const pending = this.pending.get(message.id);
    if (!pending) throw new Error("Unexpected Pi RPC response ID");
    this.pending.delete(message.id);
    clearTimeout(pending.timer);
    if (message.error) pending.reject(new RpcError(message.error.code, message.error.message));
    else pending.resolve(message.result);
  }
}

export async function connectPi(
  pontiaHome: string,
  onError: (error: Error) => void,
  onSubmit: (input: ControlInput) => void,
  models?: ModelControl,
  onReplay?: (inboxMessageId: string) => void,
  lifecycle?: LifecycleControl,
): Promise<PiConnection> {
  const path = piSocketPath(pontiaHome);
  let identity: ControlIdentity | undefined;
  let stopped = false;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let current: RpcSocket | undefined;
  const connecting = new Set<Socket>();
  const peers = new Set<RpcSocket>();
  let reconnectRejected = false;
  let attached = false;
  const waiting = new Set<() => void>();

  async function ready(): Promise<void> {
    if (!identity || attached || stopped || reconnectRejected) return;
    await new Promise<void>((resolve, reject) => {
      const wake = () => { clearTimeout(timer); waiting.delete(wake); resolve(); };
      const timer = setTimeout(() => {
        waiting.delete(wake);
        reject(new Error("Pi reconnect timed out"));
      }, REQUEST_TIMEOUT_MS);
      waiting.add(wake);
    });
  }

  function wakeRequests(): void { for (const wake of waiting) wake(); }

  async function open(reportingOnly = false): Promise<RpcSocket> {
    const socket = createConnection(path);
    connecting.add(socket);
    socket.unref();
    try {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => socket.destroy(new Error("Pi socket connection timed out")), REQUEST_TIMEOUT_MS);
        socket.once("error", reject);
        socket.once("connect", () => { clearTimeout(timer); socket.off("error", reject); resolve(); });
        socket.once("close", () => { clearTimeout(timer); reject(new Error("Pi socket closed during connect")); });
      });
    } finally { connecting.delete(socket); }
    if (stopped) { socket.destroy(); throw new Error("Pi connection is closed"); }
    const peer = new RpcSocket(socket, reportingOnly ? () => { throw new Error("Pi reporting connection cannot accept input"); } : onSubmit, reportingOnly ? undefined : models, reportingOnly ? undefined : onReplay, reportingOnly ? undefined : lifecycle);
    peers.add(peer);
    socket.once("close", () => {
      peers.delete(peer);
      if (current === peer) { current = undefined; attached = false; reconnect(); }
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
        const result = await peer.request("runtime.attach", {
          version: CONTROL_VERSION, session_id: identity!.sessionId,
          runtime_instance_id: identity!.runtimeInstanceId, client_session_key: identity!.clientSessionKey,
        }) as Record<string, unknown>;
        if (result?.session_id !== identity!.sessionId || result?.runtime_instance_id !== identity!.runtimeInstanceId) {
          throw new RpcError(-32009, "Pi reconnect identity mismatch");
        }
        attached = true;
        wakeRequests();
        await models?.onReconnect();
      } catch (error) {
        onError(error instanceof Error ? error : new Error(String(error)));
        // A rejected identity cannot be repaired by repeating registration.
        if (error instanceof RpcError && error.code !== -32603) { reconnectRejected = true; wakeRequests(); }
        current?.close();
        current = undefined;
        reconnect();
      }
    }, 250);
    retry.unref();
  }

  current = await open();
  return {
    async request(method, params) {
      if (method === "turn.startFailure") {
        if (!identity?.clientSessionKey || stopped) throw new Error("Pi reporting identity is unavailable");
        const peer = await open(true);
        try {
          return await peer.request(method, { ...params, client_session_key: identity.clientSessionKey });
        } finally { peer.close(); }
      }
      await ready();
      if (!current || stopped) return Promise.reject(new Error("Pi connection is unavailable"));
      return current.request(method, params);
    },
    registered(value) { identity = value; attached = !!current; if (!current) reconnect(); },
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
