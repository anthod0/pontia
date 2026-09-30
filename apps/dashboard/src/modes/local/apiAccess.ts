import { get } from "svelte/store";
import { token } from "./auth";

export const apiCredentials: RequestCredentials = "same-origin";
export const apiStreamUnavailableMessage =
  "Set an API token in Settings to open the dashboard event stream.";

export function apiUrl(path: string): string {
  return path;
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
