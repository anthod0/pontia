import {
  apiCredentials,
  apiFetch,
  apiSignal,
  apiUrl,
  applyApiAuthentication,
  handleApiAuthenticationFailure,
} from "$dashboard-mode/apiAccess";
import { ApiError } from "./errors";
import type { ApiEnvelope } from "./types";

export type Upload = {
  upload_id: string;
  name: string;
  size: number;
  sha256: string;
  offset: number;
  completed: boolean;
  created_at: string;
  path: string | null;
};

export async function uploadRequest(path: string, init: RequestInit = {}): Promise<Upload> {
  const headers = new Headers(init.headers);
  applyApiAuthentication(headers);
  const response = await apiFetch(apiUrl(`/api/v1/uploads${path}`), {
    ...init,
    headers,
    credentials: apiCredentials,
    signal: apiSignal(init.signal),
  });
  const envelope = (await response.json()) as ApiEnvelope<{ upload: Upload }>;
  if (!response.ok || envelope.error) {
    if (response.status === 401) handleApiAuthenticationFailure();
    throw new ApiError(
      envelope.error?.message ?? response.statusText,
      envelope.error?.code ?? "request_failed",
      response.status,
    );
  }
  if (!envelope.data) throw new Error("Upload response did not include data");
  return envelope.data.upload;
}
