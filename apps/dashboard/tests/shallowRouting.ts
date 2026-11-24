import { writable, fromStore } from "svelte/store";
import { afterEach, vi } from "vitest";

// Component tests run without SvelteKit's client router. Keep page.state in sync with jsdom history.
const shallowHistory = writable<App.PageState>({});
const pageState = fromStore(shallowHistory);
window.addEventListener("popstate", () => shallowHistory.set(history.state ?? {}));

vi.mock("$app/state", async (importOriginal) => {
  const original = await importOriginal<typeof import("$app/state")>();
  return {
    ...original,
    page: Object.create(original.page, { state: { get: () => pageState.current } }),
  };
});
vi.mock("$app/navigation", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$app/navigation")>()),
  pushState(url: string, state: App.PageState) {
    history.pushState(state, "", url || location.href);
    shallowHistory.set(state);
  },
}));

afterEach(() => {
  history.replaceState({}, "");
  shallowHistory.set({});
});
