import { hasPublicApiTarget, publicApiSignal, publicApiUrl } from "./apiTarget";

export const apiCredentials: RequestCredentials = "include";
export const apiStreamUnavailableMessage = "Remote device access is not ready.";

export function apiUrl(path: string): string {
  const developmentBridge = import.meta.env.DEV && import.meta.env.MODE === "development";
  return developmentBridge && !hasPublicApiTarget() ? path : publicApiUrl(path);
}

export function apiSignal(signal?: AbortSignal | null): AbortSignal {
  return publicApiSignal(signal);
}

export function applyApiAuthentication(headers: Headers): boolean {
  headers.delete("Authorization");
  return true;
}

export function handleApiAuthenticationFailure(): void {}
