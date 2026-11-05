import { uploadStorageScope } from "$dashboard-mode/apiAccess";
import { sharedFileContent } from "../../api/sessionFiles";
import { responseSize } from "./save";

export const PREVIEW_LIMIT = 20 * 1024 * 1024;
export const PREVIEW_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);
const previews = new Map<string, Promise<Blob | null>>();

export async function filePreview(sessionId: string, fileId: string): Promise<Blob | null> {
  const key = `${await uploadStorageScope()}:${fileId}`;
  let preview = previews.get(key);
  if (!preview) {
    preview = loadPreview(sessionId, fileId);
    previews.set(key, preview);
    preview.catch(() => previews.delete(key));
  }
  return preview;
}

async function loadPreview(sessionId: string, fileId: string): Promise<Blob | null> {
  const response = await sharedFileContent(sessionId, fileId);
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
