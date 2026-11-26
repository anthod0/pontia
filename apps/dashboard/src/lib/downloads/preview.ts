import { responseSize } from "./save";
import type { FileInfo } from "../../api/sessionFiles";
import type { DeviceRuntime } from "../../device/runtime";

export const FILE_PREVIEW_LIMIT = 5 * 1024 * 1024;

export const PREVIEW_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export function canPreviewImage(file: FileInfo): boolean {
  return file.size <= FILE_PREVIEW_LIMIT && PREVIEW_TYPES.has(file.media_type);
}

export function createPreviews(device: DeviceRuntime) {
  const previews = new Map<string, Promise<string | null>>();

  /** Object URL of an attached image, kept for the page lifetime and shared by every view of the file. */
  async function imageUrl(sessionId: string, path: string): Promise<string | null> {
    const key = JSON.stringify([await device.access.uploadStorageScope(), sessionId, path]);
    let preview = previews.get(key);
    if (!preview) {
      preview = device.fileApi
        .pathFileContent(sessionId, path)
        .then(readPreview)
        .then((blob) => blob && URL.createObjectURL(blob));
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
    if (!PREVIEW_TYPES.has(type) || (size !== null && size > FILE_PREVIEW_LIMIT)) {
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
        if (received > FILE_PREVIEW_LIMIT) return null;
        chunks.push(value.slice().buffer);
      }
    } finally {
      await reader.cancel();
      reader.releaseLock();
    }
  }
  return { imageUrl, pathImagePreview };
}
