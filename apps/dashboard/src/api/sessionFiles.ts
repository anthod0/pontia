import { ApiError } from "./errors";

import type { ApiEnvelope, SharedFile } from "./types";
import type { DeviceRuntime } from "../device/runtime";

export function createFileApi(device: DeviceRuntime) {
  async function fileRequest(path: string, signal?: AbortSignal): Promise<Response> {
    const headers = new Headers();
    device.access.applyApiAuthentication(headers);
    const response = await device.access.apiFetch(
      device.access.apiUrl(`/api/v1/sessions/${path}`),
      {
        headers,
        credentials: device.access.apiCredentials,
        signal: device.access.apiSignal(signal),
      },
    );
    if (!response.ok) {
      if (response.status === 401) device.access.handleApiAuthenticationFailure();
      const body = await response.json();
      throw new ApiError(
        body.error?.message ?? response.statusText,
        body.error?.code ?? "request_failed",
        response.status,
      );
    }
    return response;
  }

  async function listSessionFiles(sessionId: string, cursor?: string, signal?: AbortSignal) {
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

  function sharedFileContent(sessionId: string, fileId: string, signal?: AbortSignal) {
    return fileRequest(
      `${encodeURIComponent(sessionId)}/files/${encodeURIComponent(fileId)}/content`,
      signal,
    );
  }
  return { listSessionFiles, sharedFileContent };
}
