import { loadedRemoteAccess, type RemoteAccess } from "./remoteAccessLoader.svelte";

export const apiCredentials: RequestCredentials = "include";
export const apiStreamUnavailableMessage = "Remote device access is not ready.";

// The Dashboard runtime starts only after the layout has loaded remote access.
function remoteAccess(): RemoteAccess {
  const remote = loadedRemoteAccess();
  if (!remote) throw new Error(apiStreamUnavailableMessage);
  return remote;
}

export function apiUrl(path: string): string {
  return path;
}

export async function apiFetch(path: string, init: RequestInit): Promise<Response> {
  const developmentBridge = import.meta.env.DEV && import.meta.env.MODE === "development";
  const remote = remoteAccess();
  return developmentBridge && !remote.hasPublicApiTarget()
    ? fetch(path, init)
    : remote.e2eFetch(path, init);
}

export function apiSignal(signal?: AbortSignal | null): AbortSignal {
  return remoteAccess().publicApiSignal(signal);
}

export function applyApiAuthentication(headers: Headers): boolean {
  headers.delete("Authorization");
  return true;
}

export function handleApiAuthenticationFailure(): void {}
