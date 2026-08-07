import type { TurnTimelineGroup, TurnTimelineItem } from '../../api/types';

const DATABASE_NAME = 'pontia-dashboard';
const DATABASE_VERSION = 1;
const STORE_NAME = 'session-timelines';
const CACHED_TIMELINE_LIMIT = 30;
const SNAPSHOT_VERSION = 1;

export interface CachedTimelineSnapshot {
  version: typeof SNAPSHOT_VERSION;
  sessionId: string;
  mode: 'linear' | 'tree';
  groups: TurnTimelineGroup[];
  items: TurnTimelineItem[];
  nextOlderTurnId: string | null;
  latestTurnId: string | null;
  hasMore: boolean;
  cachedAt: number;
}

let databasePromise: Promise<IDBDatabase> | null = null;
let writeQueue = Promise.resolve();

export async function readCachedTimeline(sessionId: string): Promise<CachedTimelineSnapshot | null> {
  if (!sessionId || !indexedDbAvailable()) return null;
  try {
    const database = await openDatabase();
    const snapshot = await requestResult<CachedTimelineSnapshot | undefined>(
      database.transaction(STORE_NAME).objectStore(STORE_NAME).get(sessionId),
    );
    return isCachedTimelineSnapshot(snapshot, sessionId) ? snapshot : null;
  } catch {
    return null;
  }
}

export function writeCachedTimeline(snapshot: Omit<CachedTimelineSnapshot, 'version' | 'cachedAt'>): Promise<void> {
  if (!snapshot.sessionId || !indexedDbAvailable()) return Promise.resolve();
  const cachedSnapshot: CachedTimelineSnapshot = {
    ...snapshot,
    version: SNAPSHOT_VERSION,
    cachedAt: Date.now(),
  };
  const write = writeQueue.then(() => storeSnapshot(cachedSnapshot));
  writeQueue = write.catch(() => undefined);
  return write.catch(() => undefined);
}

async function storeSnapshot(snapshot: CachedTimelineSnapshot): Promise<void> {
  const database = await openDatabase();
  const writeTransaction = database.transaction(STORE_NAME, 'readwrite');
  writeTransaction.objectStore(STORE_NAME).put(snapshot);
  await transactionCompleted(writeTransaction);

  const snapshots = await requestResult<CachedTimelineSnapshot[]>(
    database.transaction(STORE_NAME).objectStore(STORE_NAME).getAll(),
  );
  const expiredSessionIds = snapshots
    .sort((left, right) => right.cachedAt - left.cachedAt || left.sessionId.localeCompare(right.sessionId))
    .slice(CACHED_TIMELINE_LIMIT)
    .map((entry) => entry.sessionId);
  if (!expiredSessionIds.length) return;

  const cleanupTransaction = database.transaction(STORE_NAME, 'readwrite');
  const store = cleanupTransaction.objectStore(STORE_NAME);
  for (const sessionId of expiredSessionIds) store.delete(sessionId);
  await transactionCompleted(cleanupTransaction);
}

function indexedDbAvailable(): boolean {
  return typeof globalThis.indexedDB !== 'undefined';
}

function openDatabase(): Promise<IDBDatabase> {
  if (databasePromise) return databasePromise;
  const opening = new Promise<IDBDatabase>((resolve, reject) => {
    const request = globalThis.indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORE_NAME)) {
        database.createObjectStore(STORE_NAME, { keyPath: 'sessionId' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Unable to open the timeline cache'));
    request.onblocked = () => reject(new Error('Timeline cache upgrade was blocked'));
  });
  const recoverableOpening = opening.catch((error) => {
    databasePromise = null;
    throw error;
  });
  databasePromise = recoverableOpening;
  return recoverableOpening;
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Timeline cache request failed'));
  });
}

function transactionCompleted(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error ?? new Error('Timeline cache transaction failed'));
    transaction.onabort = () => reject(transaction.error ?? new Error('Timeline cache transaction was aborted'));
  });
}

function isCachedTimelineSnapshot(value: unknown, sessionId: string): value is CachedTimelineSnapshot {
  if (!value || typeof value !== 'object') return false;
  const snapshot = value as Partial<CachedTimelineSnapshot>;
  return snapshot.version === SNAPSHOT_VERSION
    && snapshot.sessionId === sessionId
    && (snapshot.mode === 'linear' || snapshot.mode === 'tree')
    && Array.isArray(snapshot.groups)
    && Array.isArray(snapshot.items)
    && nullableString(snapshot.nextOlderTurnId)
    && nullableString(snapshot.latestTurnId)
    && typeof snapshot.hasMore === 'boolean'
    && typeof snapshot.cachedAt === 'number';
}

function nullableString(value: unknown): value is string | null {
  return value === null || typeof value === 'string';
}
