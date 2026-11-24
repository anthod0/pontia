import initWasm, {
  Identity,
  type Request as E2eRequest,
  Session,
} from "../../../e2e-wasm/pontia_e2e";
import wasmUrl from "../../../e2e-wasm/pontia_e2e_bg.wasm?url";
import {
  CloudRequestError,
  requestPublicDeviceConnection,
  renewDeviceAccess,
  type PublicDeviceTarget,
  type PublicDevice,
} from "./remoteAccess";
import { connectionCache } from "./sessionCache";
const CONTENT_TYPE = "application/pontia-e2e";
let initialized: Promise<unknown> | null = null;

// Edge strips JSON content types from device responses, so only its own rejection permits write replay.
async function invalidAdmission(response: Response): Promise<boolean> {
  return (
    response.status === 401 &&
    response.headers.get("Content-Type") === "application/json" &&
    (await response.json().catch(() => null))?.error === "invalid_admission_token"
  );
}

export function createRemoteConnection(
  device: Pick<PublicDevice, "handle" | "deviceId" | "edgeId">,
  userId: string,
  onConnectionFailure?: (error: unknown) => void,
) {
  const { handle } = device;
  const lifetime = new AbortController();
  let session: Session | null = null;
  let sessionTarget: PublicDeviceTarget | null = null;
  let admissionToken: string | null = null;
  let sessionLifetime = new AbortController();
  type Establishment = {
    promise: Promise<PublicDeviceTarget>;
    controller: AbortController;
    waiters: number;
  };
  let establishing: Establishment | null = null;
  let generation = 0;
  const cache = connectionCache(userId, device);
  let saved: string | null = null;

  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  let refreshing: Promise<void> | null = null;

  function scheduleRefresh(at: number) {
    clearTimeout(refreshTimer);
    const signal = sessionLifetime.signal;
    refreshTimer = setTimeout(
      () => {
        void refreshAccess().catch(() => {
          if (!signal.aborted) scheduleRefresh(Date.now() + 60_000);
        });
      },
      Math.max(0, at - Date.now()),
    );
  }

  async function renewForTarget(target: PublicDeviceTarget, signal: AbortSignal) {
    const access = await renewDeviceAccess(handle, signal);
    signal.throwIfAborted();
    if (access.userId !== userId) throw new CloudRequestError("Signed-in user changed", 401);
    if (
      access.target.deviceId !== target.deviceId ||
      access.target.edgeApiOrigin !== target.edgeApiOrigin
    )
      throw new CloudRequestError("Device changed during connection", 409);
    return access;
  }

  function refreshAccess(): Promise<void> {
    if (refreshing) return refreshing;
    if (!session || !sessionTarget) throw new Error("Device connection is unavailable");
    const active = session;
    const target = sessionTarget;
    const signal = sessionLifetime.signal;
    refreshing = renewForTarget(target, signal)
      .then((access) => {
        signal.throwIfAborted();
        admissionToken = access.admissionToken;
        saved = cache.refresh(target, admissionToken, encode(active.export()), access.refreshAt);
        scheduleRefresh(access.refreshAt);
      })
      .catch((error: unknown) => {
        if (
          !signal.aborted &&
          error instanceof CloudRequestError &&
          [401, 403, 404, 409].includes(error.status ?? 0)
        ) {
          invalidateE2eSession();
          onConnectionFailure?.(error);
        }
        throw error;
      })
      .finally(() => {
        if (!signal.aborted) refreshing = null;
      });
    return refreshing;
  }

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

  async function connectPublicDevice(signal?: AbortSignal): Promise<PublicDeviceTarget> {
    signal?.throwIfAborted();
    if (session && sessionTarget) return sessionTarget;
    if (establishing) return waitForConnection(establishing, signal);
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
      const cached = cache.read();
      if (cached) {
        try {
          session = Session.restore(decode(cached.session));
        } catch {
          cache.remove(cached.serialized);
        }
        if (session) {
          saved = cached.serialized;
          sessionTarget = cached.target;
          admissionToken = cached.token;
          scheduleRefresh(cached.refreshAt);
          storageScope = JSON.stringify([userId, cached.target.deviceId]);
          return cached.target;
        }
      }
      const identity = new Identity();
      try {
        const connection = await requestPublicDeviceConnection(
          handle,
          encode(identity.public_key()),
          connectionSignal,
        );
        checkActive();
        if (connection.userId !== userId) {
          throw new CloudRequestError("Signed-in user changed", 401);
        }
        const { target } = connection;
        if (target.deviceId !== device.deviceId) {
          throw new CloudRequestError("Device changed during connection", 409);
        }
        let access = { admissionToken: connection.admissionToken, refreshAt: connection.refreshAt };
        const handshake = identity.start(
          decode(connection.devicePublicKey),
          decode(connection.capability),
        );
        try {
          const handshakeBody = bodyBytes(handshake.bytes());
          const sendHandshake = (token: string) =>
            fetch(`${target.edgeApiOrigin}/e2e/v1/sessions`, {
              method: "POST",
              headers: { "Content-Type": CONTENT_TYPE, Authorization: `Bearer ${token}` },
              body: handshakeBody,
              signal: connectionSignal,
            });
          let confirmation = await sendHandshake(access.admissionToken);
          if (await invalidAdmission(confirmation)) {
            access = await renewForTarget(target, connectionSignal);
            confirmation = await sendHandshake(access.admissionToken);
          }
          if (!confirmation.ok) throw new Error(`E2E handshake failed (${confirmation.status})`);
          const bytes = new Uint8Array(await confirmation.arrayBuffer());
          checkActive();
          session = handshake.confirm(bytes);
          sessionTarget = target;
          admissionToken = access.admissionToken;
          storageScope = JSON.stringify([connection.userId, target.deviceId]);
          saved = cache.save(target, admissionToken, encode(session.export()), access.refreshAt);
          scheduleRefresh(access.refreshAt);
          return target;
        } finally {
          handshake.free();
        }
      } finally {
        identity.free();
      }
    })()
      .catch((error: unknown) => {
        if (!connectionSignal.aborted) onConnectionFailure?.(error);
        throw error;
      })
      .finally(() => {
        if (connectionGeneration === generation) establishing = null;
      });
    establishing = { promise, controller, waiters: 0 };
    return waitForConnection(establishing, signal);
  }

  let storageScope: string | null = null;

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

  async function encryptedBody(
    request: E2eRequest,
    body: BodyInit | null | undefined,
  ): Promise<Uint8Array> {
    let bytes: Uint8Array | undefined;
    if (typeof body === "string") bytes = new TextEncoder().encode(body);
    else if (body instanceof Blob) bytes = new Uint8Array(await body.arrayBuffer());
    else if (body instanceof ArrayBuffer) bytes = new Uint8Array(body);
    else if (ArrayBuffer.isView(body))
      bytes = new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
    else if (body != null) throw new Error("Unsupported E2E request body");
    const chunks = [request.first_bytes()];
    if (bytes?.length) chunks.push(request.content(bytes));
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

  async function e2eFetch(
    path: string,
    init: RequestInit = {},
    renewedSession = false,
    renewedToken = false,
  ): Promise<Response> {
    const signal = init.signal ? AbortSignal.any([lifetime.signal, init.signal]) : lifetime.signal;
    init = { ...init, signal };
    signal.throwIfAborted();
    const target = await connectPublicDevice(signal);
    init.signal?.throwIfAborted();
    if (!session || sessionTarget !== target)
      throw new DOMException("Connection was superseded", "AbortError");
    const active = session;
    const token = admissionToken;
    const requestSignal = AbortSignal.any([signal, sessionLifetime.signal]);
    const headers = new Headers(init.headers);
    headers.delete("authorization");
    const request = active.request(init.method ?? "GET", path, requestHeaders(headers));
    let response: Response;
    try {
      const body = bodyBytes(await encryptedBody(request, init.body));
      requestSignal.throwIfAborted();
      response = await fetch(`${target.edgeApiOrigin}/e2e/v1/requests`, {
        method: "POST",
        headers: { "Content-Type": CONTENT_TYPE, Authorization: `Bearer ${token}` },
        body,
        signal: requestSignal,
      });
    } catch (error) {
      // A network error leaves the E2E session valid; callers retry over the same session.
      request.free();
      throw error;
    }
    if (response.status === 401) {
      const method = (init.method ?? "GET").toUpperCase();
      const rejectedToken = await invalidAdmission(response);
      if (rejectedToken && !renewedToken) {
        request.free();
        signal.throwIfAborted();
        if (session === active && admissionToken === token) {
          try {
            await refreshAccess();
          } catch (error) {
            if (session === active) onConnectionFailure?.(error);
            throw error;
          }
        }
        return e2eFetch(path, init, renewedSession, true);
      }
      if (!rejectedToken && !renewedSession) {
        request.free();
        const replay = method === "GET" || method === "HEAD";
        // A delayed rejection must not invalidate a replacement session/handshake.
        if (session === active) invalidateE2eSession();
        if (replay) return e2eFetch(path, init, true, renewedToken);
        await connectPublicDevice(signal);
        throw new Error("Device connection renewed; the request was not replayed");
      }
    }
    if (response.status === 409 && !renewedSession) {
      request.free();
      // A delayed rejection must not invalidate a replacement session/handshake.
      if (session === active) invalidateE2eSession();
      return e2eFetch(path, init, true, renewedToken);
    }
    if (!response.ok) {
      request.free();
      const error = new Error(`E2E transport failed (${response.status})`);
      if (session === active) {
        // The device's per-session request budget is exhausted with a 429.
        if ([401, 409, 429].includes(response.status)) invalidateE2eSession();
        onConnectionFailure?.(error);
      }
      throw error;
    }
    return decryptResponse(response, request);
  }

  function invalidateE2eSession(): void {
    if (saved !== null) cache.remove(saved);
    clearE2eSession();
  }

  function clearE2eSession(): void {
    clearTimeout(refreshTimer);
    refreshing = null;
    generation += 1;
    establishing?.controller.abort();
    sessionLifetime.abort();
    sessionLifetime = new AbortController();
    session?.free();
    session = null;
    sessionTarget = null;
    admissionToken = null;
    storageScope = null;
    establishing = null;
    saved = null;
  }

  async function uploadStorageScope(): Promise<string> {
    await connectPublicDevice(lifetime.signal);
    if (!storageScope) throw new Error("Remote device access is not ready");
    return storageScope;
  }

  return {
    connect: (signal?: AbortSignal) =>
      connectPublicDevice(signal ? AbortSignal.any([lifetime.signal, signal]) : lifetime.signal),
    e2eFetch,
    uploadStorageScope,
    signal: lifetime.signal,
    dispose() {
      lifetime.abort();
      clearE2eSession();
    },
  };
}
