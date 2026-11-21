import type { Bot, BotEntry, BotMessagePage } from "../api/types";
import { deviceSnapshotStore } from "./deviceSnapshotStore";

const SNAPSHOT_VERSION = 1;
/** Matches the default page size, so the cache holds the latest page. */
const CACHED_BOT_ENTRY_LIMIT = 50;

export interface CachedBotTimeline {
  version: typeof SNAPSHOT_VERSION;
  deviceId: string;
  botId: string;
  bot: Bot;
  fileLinkSession: BotMessagePage["file_link_session"];
  entries: BotEntry[];
  cachedAt: number;
}

const store = deviceSnapshotStore<"botId", CachedBotTimeline>({
  database: "pontia-device-bot-timelines",
  store: "bot-timelines",
  key: "botId",
  limit: 30,
});

export async function readCachedBotTimeline(
  deviceId: string,
  botId: string,
): Promise<CachedBotTimeline | null> {
  const snapshot = await store.read(deviceId, botId);
  return isCachedBotTimeline(snapshot, deviceId, botId) ? snapshot : null;
}

export function writeCachedBotTimeline(
  snapshot: Omit<CachedBotTimeline, "version" | "cachedAt">,
): Promise<void> {
  return store.write({
    ...snapshot,
    entries: snapshot.entries.slice(-CACHED_BOT_ENTRY_LIMIT),
    version: SNAPSHOT_VERSION,
    cachedAt: Date.now(),
  });
}

export function removeCachedBotTimeline(deviceId: string, botId: string): Promise<void> {
  return store.remove(deviceId, botId);
}

function isCachedBotTimeline(
  value: unknown,
  deviceId: string,
  botId: string,
): value is CachedBotTimeline {
  if (!value || typeof value !== "object") return false;
  const snapshot = value as Partial<CachedBotTimeline>;
  return (
    snapshot.version === SNAPSHOT_VERSION &&
    snapshot.deviceId === deviceId &&
    snapshot.botId === botId &&
    typeof snapshot.bot === "object" &&
    snapshot.bot !== null &&
    snapshot.bot.bot_id === botId &&
    (snapshot.fileLinkSession === null || typeof snapshot.fileLinkSession === "object") &&
    Array.isArray(snapshot.entries) &&
    typeof snapshot.cachedAt === "number"
  );
}
