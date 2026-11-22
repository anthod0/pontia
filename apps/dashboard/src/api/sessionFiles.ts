import { ApiError } from "./errors";

import type { DeviceRuntime } from "../device/runtime";

export interface FileInfo {
  size: number;
  media_type: string;
}

export function createFileApi(device: DeviceRuntime) {
  async function fileRequest(
    sessionId: string,
    path: string,
    method: "GET" | "HEAD",
    signal?: AbortSignal,
  ): Promise<Response> {
    const headers = new Headers();
    device.access.applyApiAuthentication(headers);
    const query = new URLSearchParams({ path });
    const response = await device.access.apiFetch(
      device.access.apiUrl(
        `/api/v1/sessions/${encodeURIComponent(sessionId)}/files/content?${query}`,
      ),
      {
        method,
        headers,
        credentials: device.access.apiCredentials,
        signal: device.access.apiSignal(signal),
      },
    );
    if (!response.ok) {
      if (response.status === 401) device.access.handleApiAuthenticationFailure();
      // A HEAD response has no error body.
      const body = method === "GET" ? await response.json().catch(() => null) : null;
      throw new ApiError(
        body?.error?.message ?? response.statusText,
        body?.error?.code ?? (response.status === 404 ? "not_found" : "request_failed"),
        response.status,
      );
    }
    return response;
  }

  function pathFileContent(sessionId: string, path: string, signal?: AbortSignal) {
    return fileRequest(sessionId, path, "GET", signal);
  }

  /** Size and detected type of a local file, without reading its content. */
  async function pathFileInfo(
    sessionId: string,
    path: string,
    signal?: AbortSignal,
  ): Promise<FileInfo> {
    const response = await fileRequest(sessionId, path, "HEAD", signal);
    return {
      size: Number(response.headers.get("Content-Length") ?? 0),
      media_type: response.headers.get("Content-Type")?.split(";")[0] ?? "",
    };
  }

  return { pathFileContent, pathFileInfo };
}
