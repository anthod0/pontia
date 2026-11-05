import {
  apiCredentials,
  apiFetch,
  apiSignal,
  apiUrl,
  applyApiAuthentication,
  handleApiAuthenticationFailure,
} from "$dashboard-mode/apiAccess";
import { ApiError } from "./errors";
import type { ApiEnvelope, SharedFile } from "./types";

async function fileRequest(path: string, signal?: AbortSignal): Promise<Response> {
  const headers = new Headers();
  applyApiAuthentication(headers);
  const response = await apiFetch(apiUrl(`/api/v1/sessions/${path}`), {
    headers,
    credentials: apiCredentials,
    signal: apiSignal(signal),
  });
  if (!response.ok) {
    if (response.status === 401) handleApiAuthenticationFailure();
    const body = await response.json();
    throw new ApiError(
      body.error?.message ?? response.statusText,
      body.error?.code ?? "request_failed",
      response.status,
    );
  }
  return response;
}

export async function listSessionFiles(sessionId: string, cursor?: string, signal?: AbortSignal) {
  const response = await fileRequest(
    `${encodeURIComponent(sessionId)}/files?limit=200${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
    signal,
  );
  const body = (await response.json()) as ApiEnvelope<{
    files: SharedFile[];
    next_cursor: string | null;
  }>;
  if (!body.data) throw new Error("File list response did not include data");
  return body.data;
}

export function sharedFileContent(sessionId: string, fileId: string, signal?: AbortSignal) {
  return fileRequest(
    `${encodeURIComponent(sessionId)}/files/${encodeURIComponent(fileId)}/content`,
    signal,
  );
}
