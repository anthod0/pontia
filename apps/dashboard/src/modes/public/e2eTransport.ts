import initWasm, {
  Identity,
  type Request as E2eRequest,
  type Session,
} from "../../e2e-wasm/pontia_e2e";
import wasmUrl from "../../e2e-wasm/pontia_e2e_bg.wasm?url";
import { publicApiTarget } from "./apiTarget";
import { requestPublicDeviceConnection, type PublicDeviceTarget } from "./remoteAccess";
const CONTENT_TYPE = "application/pontia-e2e";
let session: Session | null = null;
let sessionTarget: PublicDeviceTarget | null = null;
type Establishment = {
  handle: string;
  promise: Promise<PublicDeviceTarget>;
  controller: AbortController;
  waiters: number;
};
let establishing: Establishment | null = null;
let generation = 0;
let initialized: Promise<unknown> | null = null;

function bodyBytes(value: Uint8Array): ArrayBuffer {
  return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer;
}

function decode(value: string): Uint8Array {
  const padded = value
    .replaceAll("-", "+")
    .replaceAll("_", "/")
    .padEnd(Math.ceil(value.length / 4) * 4, "=");
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function waitForConnection(
  connection: Establishment,
  signal?: AbortSignal,
): Promise<PublicDeviceTarget> {
  connection.waiters += 1;
  return new Promise((resolve, reject) => {
    let settled = false;
    const cleanup = () => {
      if (settled) return false;
      settled = true;
      signal?.removeEventListener("abort", abort);
      connection.waiters -= 1;
      return true;
    };
    const abort = () => {
      if (!cleanup()) return;
      reject(signal?.reason ?? new DOMException("Connection was aborted", "AbortError"));
      if (connection.waiters === 0 && establishing === connection) clearE2eSession();
    };
    signal?.addEventListener("abort", abort, { once: true });
    connection.promise.then(
      (target) => {
        if (cleanup()) resolve(target);
      },
      (error) => {
        if (cleanup()) reject(error);
      },
    );
    if (signal?.aborted) abort();
  });
}

export async function connectPublicDevice(
  handle: string,
  signal?: AbortSignal,
): Promise<PublicDeviceTarget> {
  signal?.throwIfAborted();
  if (session && sessionTarget?.handle === handle) return sessionTarget;
  if (establishing?.handle === handle) return waitForConnection(establishing, signal);
  if (session || establishing) clearE2eSession();
  const connectionGeneration = generation;
  const controller = new AbortController();
  const connectionSignal = controller.signal;
  const checkActive = () => {
    connectionSignal.throwIfAborted();
    if (connectionGeneration !== generation)
      throw new DOMException("Connection was superseded", "AbortError");
  };
  const promise = (async () => {
    initialized ??= initWasm(wasmUrl);
    await initialized;
    checkActive();
    const identity = new Identity();
    try {
      const connection = await requestPublicDeviceConnection(
        handle,
        encode(identity.public_key()),
        connectionSignal,
      );
      checkActive();
      const { target } = connection;
      const handshake = identity.start(
        decode(connection.devicePublicKey),
        decode(connection.capability),
      );
      try {
        const confirmation = await fetch(
          `${target.edgeApiOrigin}/devices/${target.deviceId}/e2e/v1/sessions`,
          {
            method: "POST",
            headers: { "Content-Type": CONTENT_TYPE },
            body: bodyBytes(handshake.bytes()),
            signal: connectionSignal,
          },
        );
        if (!confirmation.ok) throw new Error(`E2E handshake failed (${confirmation.status})`);
        const bytes = new Uint8Array(await confirmation.arrayBuffer());
        checkActive();
        session = handshake.confirm(bytes);
        sessionTarget = target;
        return target;
      } finally {
        handshake.free();
      }
    } finally {
      identity.free();
    }
  })().finally(() => {
    if (connectionGeneration === generation) establishing = null;
  });
  establishing = { handle, promise, controller, waiters: 0 };
  return waitForConnection(establishing, signal);
}

function encode(value: Uint8Array): string {
  let binary = "";
  for (const byte of value) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

function requestHeaders(headers: Headers): [string, string][] {
  const fields: [string, string][] = [];
  headers.forEach((value, name) => fields.push([name, value]));
  return fields;
}

function encryptedBody(request: E2eRequest, body: BodyInit | null | undefined): Uint8Array {
  if (body != null && typeof body !== "string")
    throw new Error("Public E2E requests require a text body");
  const chunks = [request.first_bytes()];
  if (body) chunks.push(request.content(new TextEncoder().encode(body)));
  chunks.push(request.finish_upload());
  const size = chunks.reduce((total, chunk) => total + chunk.length, 0);
  const output = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    output.set(chunk, offset);
    offset += chunk.length;
  }
  return output;
}

type ProtocolReader = {
  request: E2eRequest;
  reader: ReadableStreamDefaultReader<Uint8Array>;
  chunk: Uint8Array;
  offset: number;
  finished: boolean;
};

async function nextEvent(state: ProtocolReader): Promise<any[] | null> {
  while (true) {
    const event = state.request.next_event();
    if (event) return event;
    if (state.offset < state.chunk.length) {
      const consumed = state.request.receive(state.chunk.subarray(state.offset));
      if (consumed <= 0) throw new Error("E2E decoder made no progress");
      state.offset += consumed;
      continue;
    }
    const read = await state.reader.read();
    if (read.done) {
      if (!state.finished) {
        state.request.finish_response();
        state.finished = true;
      }
      return null;
    }
    state.chunk = read.value;
    state.offset = 0;
  }
}

async function decryptResponse(response: Response, request: E2eRequest): Promise<Response> {
  if (!response.body) throw new Error("E2E response body is unavailable");
  const state: ProtocolReader = {
    request,
    reader: response.body.getReader(),
    chunk: new Uint8Array(),
    offset: 0,
    finished: false,
  };
  const head = await nextEvent(state);
  if (!head || head[0] !== 0 || typeof head[1] !== "number" || !Array.isArray(head[2])) {
    throw new Error("E2E response did not contain a valid head");
  }
  const headers = new Headers(head[2] as [string, string][]);
  const body = new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        const event = await nextEvent(state);
        if (!event) throw new Error("E2E response was truncated");
        if (event[0] === 1 && event[1] instanceof Uint8Array) controller.enqueue(event[1]);
        else if (event[0] === 2) {
          if ((await nextEvent(state)) !== null)
            throw new Error("E2E response contained data after completion");
          request.free();
          controller.close();
        } else throw new Error("E2E response event was invalid");
      } catch (error) {
        request.free();
        controller.error(error);
      }
    },
    cancel() {
      void state.reader.cancel();
      request.free();
    },
  });
  return new Response(body, { status: head[1], headers });
}

export async function e2eFetch(
  path: string,
  init: RequestInit = {},
  reauthorized = false,
): Promise<Response> {
  const requestedTarget = publicApiTarget();
  const target = await connectPublicDevice(requestedTarget.handle, init.signal ?? undefined);
  init.signal?.throwIfAborted();
  if (!session || sessionTarget !== target)
    throw new DOMException("Connection was superseded", "AbortError");
  const active = session;
  const headers = new Headers(init.headers);
  headers.delete("authorization");
  const request = active.request(init.method ?? "GET", path, requestHeaders(headers));
  const response = await fetch(
    `${target.edgeApiOrigin}/devices/${target.deviceId}/e2e/v1/requests`,
    {
      method: "POST",
      headers: { "Content-Type": CONTENT_TYPE },
      body: bodyBytes(encryptedBody(request, init.body)),
      signal: init.signal,
    },
  );
  if ((response.status === 409 || response.status === 401) && !reauthorized) {
    request.free();
    // A delayed rejection must not invalidate a replacement session/handshake.
    if (session === active) clearE2eSession();
    return e2eFetch(path, init, true);
  }
  if (!response.ok) {
    request.free();
    throw new Error(`E2E transport failed (${response.status})`);
  }
  return decryptResponse(response, request);
}

export function clearE2eSession(): void {
  generation += 1;
  establishing?.controller.abort();
  session?.free();
  session = null;
  sessionTarget = null;
  establishing = null;
}
