export type UploadRecord = {
  scope: string;
  draft: string;
  key: string;
  uploadId: string | null;
  name: string;
  size: number;
  sha256: string;
};

async function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("pontia-uploads", 1);
    request.onupgradeneeded = () =>
      request.result.createObjectStore("uploads", { keyPath: ["scope", "draft", "key"] });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("Upload recovery storage is blocked"));
  });
}

async function transaction<T>(
  mode: IDBTransactionMode,
  action: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await database();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction("uploads", mode);
      const request = action(tx.objectStore("uploads"));
      tx.oncomplete = () => resolve(request.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () =>
        reject(tx.error ?? new Error("Upload recovery storage transaction aborted"));
    });
  } finally {
    db.close();
  }
}

export async function loadUploadRecords(scope: string, draft: string): Promise<UploadRecord[]> {
  const records = await transaction("readonly", (store) =>
    store.getAll(IDBKeyRange.bound([scope, draft], [scope, draft, []])),
  );
  return (records as UploadRecord[]).filter(
    (record) => localStorage.getItem(removalKey(record)) === null,
  );
}
export async function saveUploadRecord(record: UploadRecord): Promise<void> {
  await transaction("readwrite", (store) => store.put(record));
}
function removalKey(record: UploadRecord): string {
  return `pontia-upload-removed:${JSON.stringify([record.scope, record.draft, record.key])}`;
}

export async function removeUploadRecord(record: UploadRecord): Promise<void> {
  try {
    await transaction("readwrite", (store) =>
      store.delete([record.scope, record.draft, record.key]),
    );
  } catch {
    // A durable marker prevents a sent or removed reference from returning if IndexedDB fails.
    localStorage.setItem(removalKey(record), "1");
  }
}
