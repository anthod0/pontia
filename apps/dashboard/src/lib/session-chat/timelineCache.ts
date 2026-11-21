import type { TurnTimelineGroup, TurnTimelineItem } from "../../api/types";
import { deviceSnapshotStore } from "../deviceSnapshotStore";

const SNAPSHOT_VERSION = 3;

export interface CachedTimelineSnapshot {
  version: typeof SNAPSHOT_VERSION;
  deviceId: string;
  sessionId: string;
  mode: "linear" | "tree";
  historySource?: "native" | "file" | null;
  groups: TurnTimelineGroup[];
  items: TurnTimelineItem[];
  nextOlderTurnId: string | null;
  latestTurnId: string | null;
  hasMore: boolean;
  cachedAt: number;
}

const store = deviceSnapshotStore<"sessionId", CachedTimelineSnapshot>({
  database: "pontia-device-timelines",
  store: "session-timelines",
  key: "sessionId",
  limit: 30,
});

export async function readCachedTimeline(
  deviceId: string,
  sessionId: string,
): Promise<CachedTimelineSnapshot | null> {
  if (!sessionId) return null;
  const snapshot = await store.read(deviceId, sessionId);
  return isCachedTimelineSnapshot(snapshot, deviceId, sessionId) ? snapshot : null;
}

export function writeCachedTimeline(
  snapshot: Omit<CachedTimelineSnapshot, "version" | "cachedAt">,
): Promise<void> {
  if (!snapshot.sessionId) return Promise.resolve();
  return store.write({ ...snapshot, version: SNAPSHOT_VERSION, cachedAt: Date.now() });
}

function isCachedTimelineSnapshot(
  value: unknown,
  deviceId: string,
  sessionId: string,
): value is CachedTimelineSnapshot {
  if (!value || typeof value !== "object") return false;
  const snapshot = value as Partial<CachedTimelineSnapshot>;
  return (
    snapshot.version === SNAPSHOT_VERSION &&
    snapshot.deviceId === deviceId &&
    snapshot.sessionId === sessionId &&
    (snapshot.mode === "linear" || snapshot.mode === "tree") &&
    Array.isArray(snapshot.groups) &&
    Array.isArray(snapshot.items) &&
    nullableString(snapshot.nextOlderTurnId) &&
    nullableString(snapshot.latestTurnId) &&
    typeof snapshot.hasMore === "boolean" &&
    typeof snapshot.cachedAt === "number"
  );
}

function nullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}
