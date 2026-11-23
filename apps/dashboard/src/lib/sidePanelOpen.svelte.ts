import { MediaQuery } from "svelte/reactivity";
import { PersistedState } from "./storage";

const openSerializer = {
  parse: (saved: string) => saved === "true",
  stringify: (open: boolean) => (open ? "true" : null),
};

/**
 * Whether a right-hand side panel is open. On wide screens it is saved per browser under `key`,
 * so the panel stays open or closed across pages and reloads; on phones the full-screen panel
 * stays closed until it is opened.
 */
export class SidePanelOpen {
  #saved: PersistedState<boolean>;
  #phoneOpen = $state(false);
  #wide = new MediaQuery("min-width: 1024px");

  constructor(key: string) {
    this.#saved = new PersistedState(key, false, openSerializer);
  }

  /** Whether the panel sits beside the page instead of covering it. */
  get wide(): boolean {
    return this.#wide.current;
  }

  get current(): boolean {
    return this.wide ? this.#saved.current : this.#phoneOpen;
  }

  set current(open: boolean) {
    if (this.wide) this.#saved.current = open;
    else this.#phoneOpen = open;
  }
}
