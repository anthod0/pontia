import { hasPublicApiTarget, publicApiSignal } from "./apiTarget";
import { e2eFetch } from "./e2eTransport";

export const apiCredentials: RequestCredentials = "include";
export const apiStreamUnavailableMessage = "Remote device access is not ready.";

export function apiUrl(path: string): string {
  return path;
}

export function apiFetch(path: string, init: RequestInit): Promise<Response> {
  const developmentBridge = import.meta.env.DEV && import.meta.env.MODE === "development";
  return developmentBridge && !hasPublicApiTarget() ? fetch(path, init) : e2eFetch(path, init);
}

export function apiSignal(signal?: AbortSignal | null): AbortSignal {
  return publicApiSignal(signal);
}

export function applyApiAuthentication(headers: Headers): boolean {
  headers.delete("Authorization");
  return true;
}

export function handleApiAuthenticationFailure(): void {}
