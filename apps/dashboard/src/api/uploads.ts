import { ApiError } from "./errors";

import type { ApiEnvelope } from "./types";
import type { DeviceRuntime } from "../device/runtime";

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
export function createUploads(device: DeviceRuntime) {
  async function uploadRequest(path: string, init: RequestInit = {}): Promise<Upload> {
    const headers = new Headers(init.headers);
    device.access.applyApiAuthentication(headers);
    const response = await device.access.apiFetch(device.access.apiUrl(`/api/v1/uploads${path}`), {
      ...init,
      headers,
      credentials: device.access.apiCredentials,
      signal: device.access.apiSignal(init.signal),
    });
    const envelope = (await response.json()) as ApiEnvelope<{ upload: Upload }>;
    if (!response.ok || envelope.error) {
      if (response.status === 401) device.access.handleApiAuthenticationFailure();
      throw new ApiError(
        envelope.error?.message ?? response.statusText,
        envelope.error?.code ?? "request_failed",
        response.status,
      );
    }
    if (!envelope.data) throw new Error("Upload response did not include data");
    return envelope.data.upload;
  }
  return { uploadRequest };
}
