import { responseSize } from "./save";
import type { DeviceRuntime } from "../../device/runtime";

export const PREVIEW_LIMIT = 20 * 1024 * 1024;

export const PREVIEW_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);
export function createPreviews(device: DeviceRuntime) {
  const previews = new Map<string, Promise<Blob | null>>();

  async function filePreview(sessionId: string, fileId: string): Promise<Blob | null> {
    const key = `${await device.access.uploadStorageScope()}:${fileId}`;
    let preview = previews.get(key);
    if (!preview) {
      preview = device.fileApi.sharedFileContent(sessionId, fileId).then(readPreview);
      previews.set(key, preview);
      preview.catch(() => previews.delete(key));
    }
    return preview;
  }

  /** Reads a local image by path. Not cached because the file can change on disk. */
  async function pathImagePreview(
    sessionId: string,
    path: string,
    signal?: AbortSignal,
  ): Promise<Blob | null> {
    return readPreview(await device.fileApi.pathFileContent(sessionId, path, signal));
  }

  async function readPreview(response: Response): Promise<Blob | null> {
    const type = response.headers.get("Content-Type")?.split(";")[0] ?? "";
    const size = responseSize(response);
    if (!PREVIEW_TYPES.has(type) || (size !== null && size > PREVIEW_LIMIT)) {
      await response.body?.cancel();
      return null;
    }
    const reader = response.body!.getReader();
    let received = 0;
    const chunks: ArrayBuffer[] = [];
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) return new Blob(chunks, { type });
        received += value.byteLength;
        if (received > PREVIEW_LIMIT) return null;
        chunks.push(value.slice().buffer);
      }
    } finally {
      await reader.cancel();
      reader.releaseLock();
    }
  }
  return { filePreview, pathImagePreview };
}
