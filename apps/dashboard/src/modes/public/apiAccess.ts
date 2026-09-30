export const apiCredentials: RequestCredentials = "include";
export const apiStreamUnavailableMessage = "Remote device access is not ready.";

export function applyApiAuthentication(headers: Headers): boolean {
  headers.delete("Authorization");
  return true;
}

export function handleApiAuthenticationFailure(): void {}
