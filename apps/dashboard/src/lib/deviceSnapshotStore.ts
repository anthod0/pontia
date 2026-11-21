/** An IndexedDB store of per-device snapshots, keyed by device and one ID field. */
export interface DeviceSnapshot {
  deviceId: string;
  cachedAt: number;
}

export interface DeviceSnapshotStore<T extends DeviceSnapshot> {
  /** Resolves null when IndexedDB is unavailable or the read fails; callers validate the value. */
  read(deviceId: string, id: string): Promise<unknown>;
  /** Stores a snapshot, then drops the device's oldest snapshots beyond the limit. */
  write(snapshot: T): Promise<void>;
  remove(deviceId: string, id: string): Promise<void>;
}

function indexedDbAvailable(): boolean {
  return typeof globalThis.indexedDB !== "undefined";
}

export function deviceSnapshotStore<
  K extends string,
  T extends DeviceSnapshot & Record<K, string>,
>({
  database: databaseName,
  store: storeName,
  key,
  limit,
}: {
  database: string;
  store: string;
  key: K;
  limit: number;
}): DeviceSnapshotStore<T> {
  let databasePromise: Promise<IDBDatabase> | null = null;
  let writeQueue = Promise.resolve();

  function openDatabase(): Promise<IDBDatabase> {
    if (databasePromise) return databasePromise;
    const opening = new Promise<IDBDatabase>((resolve, reject) => {
      const request = globalThis.indexedDB.open(databaseName, 1);
      request.onupgradeneeded = () => {
        const database = request.result;
        if (!database.objectStoreNames.contains(storeName)) {
          database.createObjectStore(storeName, { keyPath: ["deviceId", key] });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () =>
        reject(request.error ?? new Error("Unable to open the timeline cache"));
      request.onblocked = () => reject(new Error("Timeline cache upgrade was blocked"));
    });
    const recoverableOpening = opening.catch((error) => {
      databasePromise = null;
      throw error;
    });
    databasePromise = recoverableOpening;
    return recoverableOpening;
  }

  function enqueue(task: () => Promise<void>): Promise<void> {
    if (!indexedDbAvailable()) return Promise.resolve();
    const write = writeQueue.then(task);
    writeQueue = write.catch(() => undefined);
    return write.catch(() => undefined);
  }

  async function store(snapshot: T): Promise<void> {
    const database = await openDatabase();
    const writeTransaction = database.transaction(storeName, "readwrite");
    writeTransaction.objectStore(storeName).put(snapshot);
    await transactionCompleted(writeTransaction);

    const snapshots = await requestResult<T[]>(
      database.transaction(storeName).objectStore(storeName).getAll(),
    );
    const expiredIds = snapshots
      .filter((entry) => entry.deviceId === snapshot.deviceId)
      .sort((left, right) => right.cachedAt - left.cachedAt || left[key].localeCompare(right[key]))
      .slice(limit)
      .map((entry) => entry[key]);
    if (!expiredIds.length) return;

    const cleanupTransaction = database.transaction(storeName, "readwrite");
    const objectStore = cleanupTransaction.objectStore(storeName);
    for (const id of expiredIds) objectStore.delete([snapshot.deviceId, id]);
    await transactionCompleted(cleanupTransaction);
  }

  return {
    async read(deviceId, id) {
      if (!indexedDbAvailable()) return null;
      try {
        const database = await openDatabase();
        return await requestResult<unknown>(
          database.transaction(storeName).objectStore(storeName).get([deviceId, id]),
        );
      } catch {
        return null;
      }
    },
    write: (snapshot) => enqueue(() => store(snapshot)),
    remove: (deviceId, id) =>
      enqueue(async () => {
        const transaction = (await openDatabase()).transaction(storeName, "readwrite");
        transaction.objectStore(storeName).delete([deviceId, id]);
        await transactionCompleted(transaction);
      }),
  };
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("Timeline cache request failed"));
  });
}

function transactionCompleted(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () =>
      reject(transaction.error ?? new Error("Timeline cache transaction failed"));
    transaction.onabort = () =>
      reject(transaction.error ?? new Error("Timeline cache transaction was aborted"));
  });
}
