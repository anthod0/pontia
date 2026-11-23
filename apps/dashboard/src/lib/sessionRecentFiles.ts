import type { RecentFile } from "./botRecentFiles";
import { PersistedState, scopedKey } from "./storage";

// One list per device holds every Session's files, so the keys do not grow with each Session.
const recentFileLimit = 50;
const sessionFileLimit = 10;

function sameFile(a: RecentFile, b: RecentFile): boolean {
  return a.sessionId === b.sessionId && a.path === b.path;
}

/** Puts `file` first. A file already listed moves up instead of appearing twice. */
export function withSessionRecentFile(files: RecentFile[], file: RecentFile): RecentFile[] {
  return [file, ...files.filter((entry) => !sameFile(entry, file))].slice(0, recentFileLimit);
}

/** The files opened in one Session, newest first. */
export function sessionRecentFiles(files: RecentFile[], sessionId: string): RecentFile[] {
  return files.filter((entry) => entry.sessionId === sessionId).slice(0, sessionFileLimit);
}

/** Forgets the files opened in one Session. */
export function withoutSessionRecentFiles(files: RecentFile[], sessionId: string): RecentFile[] {
  return files.filter((entry) => entry.sessionId !== sessionId);
}

// The files recently opened in session side panels survive reloads, per device.
export function createSessionRecentFiles(deviceId: string): PersistedState<RecentFile[]> {
  return new PersistedState<RecentFile[]>(scopedKey("pontia.sessionRecentFiles", deviceId), [], {
    parse: (saved) => JSON.parse(saved) as RecentFile[],
    stringify: (files) => (files.length ? JSON.stringify(files) : null),
  });
}
