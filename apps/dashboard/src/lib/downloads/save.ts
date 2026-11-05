const MEMORY_LIMIT = 64 * 1024 * 1024;
const DOWNLOAD_DIRECTORY = "pontia-downloads";
const LOCK_PREFIX = "pontia-download:";
const MAX_SIZE = 1024 * 1024 * 1024;

export type DownloadProgress = { received: number; total: number | null };

export function responseSize(response: Response): number | null {
  const value = response.headers.get("Content-Length");
  return value !== null && /^\d+$/.test(value) ? Number(value) : null;
}

function retentionMilliseconds(size: number): number {
  return Math.max(60_000, (size / (10 * 1024 * 1024)) * 1000);
}

function triggerSave(blob: Blob, name: string, retention: number) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), retention);
}

async function downloadRoot(): Promise<FileSystemDirectoryHandle | null> {
  if (!navigator.storage?.getDirectory || !navigator.locks) return null;
  try {
    return await (
      await navigator.storage.getDirectory()
    ).getDirectoryHandle(DOWNLOAD_DIRECTORY, { create: true });
  } catch {
    return null;
  }
}

async function setExpiry(dir: FileSystemDirectoryHandle, expires: number) {
  const handle = await dir.getFileHandle("expires", { create: true });
  const writer = await handle.createWritable();
  await writer.write(String(expires));
  await writer.close();
}

async function cleanupEntry(root: FileSystemDirectoryHandle, id: string) {
  await navigator.locks.request(LOCK_PREFIX + id, { ifAvailable: true }, async (lock) => {
    if (!lock) return;
    let expires = 0;
    try {
      const dir = await root.getDirectoryHandle(id);
      expires = Number(await (await (await dir.getFileHandle("expires")).getFile()).text());
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "NotFoundError")) throw error;
    }
    if (expires > Date.now()) return;
    try {
      await root.removeEntry(id, { recursive: true });
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "NotFoundError")) throw error;
    }
  });
}

export async function cleanupDownloads(): Promise<void> {
  const root = await downloadRoot();
  if (!root) return;
  const entries = root as FileSystemDirectoryHandle & {
    keys(): AsyncIterableIterator<string>;
  };
  for await (const id of entries.keys()) await cleanupEntry(root, id);
}

async function receive(
  response: Response,
  onProgress: (progress: DownloadProgress) => void,
  writer?: FileSystemWritableFileStream,
): Promise<{ blob: Blob; size: number }> {
  const total = responseSize(response);
  if (total !== null && total > MAX_SIZE) throw new Error("File exceeds the 1 GiB download limit");
  const chunks: ArrayBuffer[] = [];
  let received = 0;
  onProgress({ received, total });
  const reader = response.body!.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > MAX_SIZE) throw new Error("File exceeds the 1 GiB download limit");
      if (writer) await writer.write(value as Uint8Array<ArrayBuffer>);
      else chunks.push(value.slice().buffer);
      onProgress({ received, total });
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
  return { blob: new Blob(chunks, { type: "application/octet-stream" }), size: received };
}

export async function saveDownload(
  response: Response,
  name: string,
  onProgress: (progress: DownloadProgress) => void,
): Promise<void> {
  const total = responseSize(response);
  const root = total === null || total > MEMORY_LIMIT ? await downloadRoot() : null;
  if (root) {
    const id = crypto.randomUUID();
    const saved = await navigator.locks.request(LOCK_PREFIX + id, async () => {
      let dir: FileSystemDirectoryHandle;
      let handle: FileSystemFileHandle;
      let writer: FileSystemWritableFileStream;
      try {
        dir = await root.getDirectoryHandle(id, { create: true });
        await setExpiry(dir, 0);
        handle = await dir.getFileHandle("content", { create: true });
        writer = await handle.createWritable();
      } catch {
        // No response bytes have been consumed; use the same response in memory.
        return false;
      }
      try {
        const { size } = await receive(response, onProgress, writer);
        await writer.close();
        const retention = retentionMilliseconds(size);
        const file = await handle.getFile();
        triggerSave(file, name, retention);
        await setExpiry(dir, Date.now() + retention);
        setTimeout(() => void cleanupEntry(root, id).catch(console.error), retention);
        return true;
      } catch (error) {
        await writer.abort().catch(() => {});
        throw error;
      }
    });
    if (saved) return;
    await cleanupEntry(root, id);
  }
  const { blob, size } = await receive(response, onProgress);
  triggerSave(blob, name, retentionMilliseconds(size));
}
