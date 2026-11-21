import { ApiError } from "../api/errors";
import type { DeviceRuntime } from "../device/runtime";

// A newly opened tab or app window that lands on a device's New Chat page returns
// to the last Session chat or Bot chat viewed on that device. Reloads and links do not.
export type LastPage = { kind: "chat" | "bot"; id: string };

const tabKey = "pontia.tabOpened";
let entryPath: string | null = null;

function key(deviceId: string): string {
  return `pontia.lastPage:${deviceId}`;
}

function trimSlash(path: string): string {
  return path.replace(/\/+$/, "");
}

function read(deviceId: string): LastPage | null {
  try {
    const saved = localStorage.getItem(key(deviceId));
    return saved ? (JSON.parse(saved) as LastPage) : null;
  } catch {
    return null;
  }
}

function forget(deviceId: string): void {
  try {
    localStorage.removeItem(key(deviceId));
  } catch {
    // Storage is optional.
  }
}

/** Records the path a tab opened on, unless it already showed the Dashboard before a reload. */
export function openDashboardTab(): void {
  try {
    if (!sessionStorage.getItem(tabKey)) entryPath = trimSlash(location.pathname);
    sessionStorage.setItem(tabKey, "1");
  } catch {
    // Storage is optional.
  }
}

export function rememberLastPage(deviceId: string, page: LastPage): void {
  try {
    localStorage.setItem(key(deviceId), JSON.stringify(page));
  } catch {
    // Storage is optional.
  }
}

export function forgetLastPage(deviceId: string, page: LastPage): void {
  const saved = read(deviceId);
  if (saved?.kind === page.kind && saved.id === page.id) forget(deviceId);
}

/** The page to reopen, at most once per tab, when the tab opened on this New Chat page without a query. */
export function takeLastPage(deviceId: string): LastPage | null {
  const entry = entryPath === trimSlash(location.pathname) && !location.search && !location.hash;
  entryPath = null;
  return entry ? read(deviceId) : null;
}

/** The route to reopen, or null after forgetting a Session or Bot that no longer exists. */
export async function lastPagePath(device: DeviceRuntime, page: LastPage): Promise<string | null> {
  const path = `/${page.kind === "chat" ? "chat" : "bots"}/${encodeURIComponent(page.id)}`;
  try {
    if (page.kind === "chat") {
      await device.api.getSession(page.id);
      return path;
    }
    const bots = await device.queryClient.fetchQuery({
      queryKey: ["bots"],
      queryFn: ({ signal }) => device.api.listBots(signal),
    });
    if (bots.some((bot) => bot.bot_id === page.id)) return path;
  } catch (error) {
    // Other failures open the page, which reports them and offers a retry.
    if (!(error instanceof ApiError && error.status === 404)) return path;
  }
  forget(device.id);
  return null;
}
