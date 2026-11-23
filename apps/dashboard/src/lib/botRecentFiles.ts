import { PersistedState, scopedKey } from "./storage";

/** A file opened in a side panel, read through the Session it was opened from. */
export type RecentFile = { sessionId: string; path: string };

const recentFileLimit = 10;

/** Puts `file` first. A path already in the list moves up instead of appearing twice. */
export function withRecentFile(files: RecentFile[], file: RecentFile): RecentFile[] {
  return [file, ...files.filter((entry) => entry.path !== file.path)].slice(0, recentFileLimit);
}

// The files recently opened in a Bot chat's side panel survive reloads, per device and Bot.
export function createRecentFiles(deviceId: string, botId: string): PersistedState<RecentFile[]> {
  return new PersistedState<RecentFile[]>(scopedKey("pontia.botRecentFiles", deviceId, botId), [], {
    parse: (saved) => JSON.parse(saved) as RecentFile[],
    stringify: (files) => (files.length ? JSON.stringify(files) : null),
  });
}
