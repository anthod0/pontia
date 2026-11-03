import { sha256 } from "@noble/hashes/sha2.js";
import { get } from "svelte/store";
import { token } from "./auth";

export const apiCredentials: RequestCredentials = "same-origin";
export const apiStreamUnavailableMessage =
  "Set an API token in Settings to open the dashboard event stream.";

export function apiUrl(path: string): string {
  return path;
}

export function apiFetch(path: string, init: RequestInit): Promise<Response> {
  return fetch(path, init);
}

export function apiSignal(signal?: AbortSignal | null): AbortSignal | undefined {
  return signal ?? undefined;
}

export function applyApiAuthentication(headers: Headers): boolean {
  const bearer = get(token).trim();
  if (!bearer) return false;
  headers.set("Authorization", `Bearer ${bearer}`);
  return true;
}

export function handleApiAuthenticationFailure(): void {
  token.set("");
}

export async function uploadStorageScope(): Promise<string> {
  const digest = sha256(new TextEncoder().encode(get(token)));
  const identity = Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  return JSON.stringify([location.origin, identity]);
}
