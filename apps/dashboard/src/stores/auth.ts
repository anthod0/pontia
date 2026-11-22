import { writable, type Writable } from "svelte/store";
import { readStored, writeStored } from "$lib/storage";

const storageKey = "pontia.externalApiToken";
const value = writable("");

// An empty token is saved as "" rather than removed.
function persist(next: string): void {
  writeStored(storageKey, next);
}

export const token: Writable<string> = {
  subscribe: value.subscribe,
  set(next) {
    persist(next);
    value.set(next);
  },
  update(updater) {
    value.update((current) => {
      const next = updater(current);
      persist(next);
      return next;
    });
  },
};

export function loadTokenFromStorage(): string {
  const stored = readStored(storageKey) ?? "";
  value.set(stored);
  return stored;
}

export function consumeTokenFromUrl(): void {
  if (typeof window === "undefined") return;

  const url = new URL(window.location.href);
  if (!url.searchParams.has("token")) return;

  const urlToken = url.searchParams.get("token")?.trim() ?? "";
  if (urlToken) token.set(urlToken);

  url.searchParams.delete("token");
  window.history.replaceState(window.history.state, "", `${url.pathname}${url.search}${url.hash}`);
}
