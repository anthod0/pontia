export interface ApiAccess {
  apiCredentials: RequestCredentials;
  apiStreamUnavailableMessage: string;
  apiUrl(path: string): string;
  apiFetch(path: string, init: RequestInit): Promise<Response>;
  apiSignal(signal?: AbortSignal | null): AbortSignal;
  applyApiAuthentication(headers: Headers): boolean;
  handleApiAuthenticationFailure(): void;
  uploadStorageScope(): Promise<string>;
  dispose(): void;
}
