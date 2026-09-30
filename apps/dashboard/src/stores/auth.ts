import { writable, type Writable } from "svelte/store";

const storageKey = "pontia.externalApiToken";
const value = writable("");

function persist(next: string): void {
  if (typeof localStorage !== "undefined") localStorage.setItem(storageKey, next);
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
  const stored =
    typeof localStorage === "undefined" ? "" : (localStorage.getItem(storageKey) ?? "");
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
