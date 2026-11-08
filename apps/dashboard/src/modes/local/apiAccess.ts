import { sha256 } from "@noble/hashes/sha2.js";
import { get } from "svelte/store";
import { token } from "./auth";

import type { ApiAccess } from "../../device/apiAccess";

export function createApiAccess(): ApiAccess {
  const credential = get(token);
  const lifetime = new AbortController();
  const apiCredentials: RequestCredentials = "same-origin";
  const apiStreamUnavailableMessage =
    "Set an API token in Settings to open the dashboard event stream.";

  function apiUrl(path: string): string {
    return path;
  }

  function apiFetch(path: string, init: RequestInit): Promise<Response> {
    lifetime.signal.throwIfAborted();
    return fetch(path, init);
  }

  function apiSignal(signal?: AbortSignal | null): AbortSignal {
    return signal ? AbortSignal.any([lifetime.signal, signal]) : lifetime.signal;
  }

  function applyApiAuthentication(headers: Headers): boolean {
    const bearer = credential.trim();
    if (!bearer) return false;
    headers.set("Authorization", `Bearer ${bearer}`);
    return true;
  }

  function handleApiAuthenticationFailure(): void {
    if (!lifetime.signal.aborted && get(token) === credential) token.set("");
  }

  async function uploadStorageScope(): Promise<string> {
    const digest = sha256(new TextEncoder().encode(credential));
    const identity = Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    return JSON.stringify([location.origin, identity]);
  }

  return {
    apiCredentials,
    apiStreamUnavailableMessage,
    apiUrl,
    apiFetch,
    apiSignal,
    applyApiAuthentication,
    handleApiAuthenticationFailure,
    uploadStorageScope,
    dispose: () => lifetime.abort(),
  };
}
