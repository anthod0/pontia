import { writable, fromStore } from "svelte/store";
import { afterEach, vi } from "vitest";

// Component tests run without SvelteKit's client router. Keep page.state and page.url in sync with jsdom history.
const shallowHistory = writable<App.PageState>({});
const pageState = fromStore(shallowHistory);
const href = writable(location.href);
const pageHref = fromStore(href);
function sync(state: App.PageState) {
  shallowHistory.set(state);
  href.set(location.href);
}
window.addEventListener("popstate", () => sync(history.state ?? {}));

vi.mock("$app/state", async (importOriginal) => {
  const original = await importOriginal<typeof import("$app/state")>();
  return {
    ...original,
    page: Object.create(original.page, {
      state: { get: () => pageState.current },
      // Tests may set the location directly; history changes made here also notify readers.
      url: { get: () => (pageHref.current, new URL(location.href)) },
    }),
  };
});
vi.mock("$app/navigation", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$app/navigation")>()),
  pushState(url: string, state: App.PageState) {
    history.pushState(state, "", url || location.href);
    sync(state);
  },
  async goto(url: string, options: { replaceState?: boolean; state?: App.PageState } = {}) {
    const state = options.state ?? {};
    if (options.replaceState) history.replaceState(state, "", url);
    else history.pushState(state, "", url);
    sync(state);
  },
}));

afterEach(() => {
  history.replaceState({}, "", location.pathname);
  sync({});
});
