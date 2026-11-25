import { goto } from "$app/navigation";
import { page } from "$app/state";
import { MediaQuery } from "svelte/reactivity";

/** The query parameter that holds the open side panel's tab. Without it the panel is closed. */
export const sidePanelParam = "panel";

/**
 * A right-hand side panel whose open state and tab live in the page URL, as `?panel=<tab>`,
 * so each page keeps its own panel across reloads and history. Wide screens replace the
 * history entry; the overlay on narrower screens adds one, so Back closes it.
 */
export class SidePanelRoute<Tab extends string> {
  #tabs: readonly Tab[];
  /** The tab to open on; it follows the tab last chosen on this page. */
  #last: Tab;
  #closing: Promise<void> | null = null;
  #wide = new MediaQuery("min-width: 1024px");

  constructor(tabs: readonly [Tab, ...Tab[]]) {
    this.#tabs = tabs;
    this.#last = $state(tabs[0]);
  }

  /** Whether the panel sits beside the page instead of covering it. */
  get wide(): boolean {
    return this.#wide.current;
  }

  get open(): boolean {
    return page.url.searchParams.has(sidePanelParam);
  }

  /** The tab in the URL; an unknown one shows the tab last chosen. */
  get tab(): Tab {
    const value = page.url.searchParams.get(sidePanelParam);
    return this.#tabs.find((tab) => tab === value) ?? this.#last;
  }

  /** Chooses the tab, and shows it when the panel is open. */
  set tab(tab: Tab) {
    this.#last = tab;
    if (this.open) void this.show(tab);
  }

  /** Opens the panel on `tab`, or switches the open panel to it. */
  show(tab: Tab = this.tab): Promise<void> {
    this.#last = tab;
    if (this.#closing) return this.#closing;
    if (this.open && page.url.searchParams.get(sidePanelParam) === tab) return Promise.resolve();
    const push = !this.open && !this.wide;
    return this.#go(tab, push ? { ...page.state, sidePanel: true } : page.state, !push);
  }

  /** Closes the panel. The promise settles once the URL no longer opens it. */
  close(): Promise<void> {
    if (this.#closing) return this.#closing;
    if (!this.open) return Promise.resolve();
    this.#last = this.tab;
    if (!page.state.sidePanel) return this.#go(null, page.state, true);
    // The overlay added this history entry, so closing it goes back instead of adding another.
    const closing = (this.#closing = new Promise<void>((resolve) => {
      const stop = $effect.root(() => {
        $effect(() => {
          if (this.open) return;
          this.#closing = null;
          resolve();
          queueMicrotask(stop);
        });
      });
    }));
    history.back();
    return closing;
  }

  #go(tab: Tab | null, state: App.PageState, replaceState: boolean): Promise<void> {
    // Some code edits the address directly, which page.url does not see.
    const url = new URL(location.href);
    if (tab) url.searchParams.set(sidePanelParam, tab);
    else url.searchParams.delete(sidePanelParam);
    return goto(`${url.pathname}${url.search}${url.hash}`, {
      replaceState,
      state,
      keepFocus: true,
      noScroll: true,
    });
  }
}
