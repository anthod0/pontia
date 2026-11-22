import { ApiError } from "../api/errors";
import type { DeviceRuntime } from "../device/runtime";
import { readStored, readStoredWith, scopedKey, writeStored } from "./storage";

// A newly opened tab or app window that lands on a device's New Chat page returns
// to the last Session chat or Bot chat viewed on that device, and so does "Back to chat".
// Reloads and links do not.
export type LastPage = { kind: "chat" | "bot"; id: string };

const tabKey = "pontia.tabOpened";
let entryPath: string | null = null;
// A "Back to chat" request waits for the next navigation and then belongs to the New Chat path it reached.
// It ends once a mounted New Chat page has resolved it, or when the user moves to another path.
let returnPending = false;
let returnPath: string | null = null;

function key(deviceId: string): string {
  return scopedKey("pontia.lastPage", deviceId);
}

function trimSlash(path: string): string {
  return path.replace(/\/+$/, "");
}

// Saved pages are read once when needed, not kept as state.
function read(deviceId: string): LastPage | null {
  return readStoredWith(key(deviceId), (saved) => JSON.parse(saved) as LastPage);
}

function forget(deviceId: string): void {
  writeStored(key(deviceId), null);
}

function arriveAt(path: string): void {
  if (returnPending) returnPath = path;
  else if (path !== returnPath) returnPath = null;
  returnPending = false;
}

// `navigate` and history traversal end with popstate.
window.addEventListener("popstate", () => arriveAt(trimSlash(location.pathname)));

/** Records the path a tab opened on, unless it already showed the Dashboard before a reload. */
export function openDashboardTab(): void {
  if (!readStored(tabKey, { area: "session" })) entryPath = trimSlash(location.pathname);
  writeStored(tabKey, "1", { area: "session" });
}

export function rememberLastPage(deviceId: string, page: LastPage): void {
  writeStored(key(deviceId), JSON.stringify(page));
}

export function forgetLastPage(deviceId: string, page: LastPage): void {
  const saved = read(deviceId);
  if (saved?.kind === page.kind && saved.id === page.id) forget(deviceId);
}

/** Makes the New Chat page that the next navigation opens reopen its device's last page. */
export function returnToLastPage(): void {
  returnPending = true;
  returnPath = null;
}

/** Ends the "Back to chat" request once a New Chat page has opened its target or stayed on New Chat. */
export function endReturnToLastPage(): void {
  returnPath = null;
}

/** The page to reopen after "Back to chat", or once per tab when the tab opened on this New Chat page without a query. */
export function takeLastPage(deviceId: string): LastPage | null {
  const path = trimSlash(location.pathname);
  arriveAt(path);
  const entry = returnPath === path || (entryPath === path && !location.search && !location.hash);
  entryPath = null;
  const page = entry ? read(deviceId) : null;
  if (!page) endReturnToLastPage();
  return page;
}

/** The route to reopen, or null after forgetting a Bot that was deleted or a Session that is gone or archived. */
export async function lastPagePath(device: DeviceRuntime, page: LastPage): Promise<string | null> {
  const path = `/${page.kind === "chat" ? "chat" : "bots"}/${encodeURIComponent(page.id)}`;
  try {
    if (page.kind === "chat") {
      const session = await device.api.getSession(page.id);
      if (!session.archived_at) return path;
      forget(device.id);
      return null;
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
