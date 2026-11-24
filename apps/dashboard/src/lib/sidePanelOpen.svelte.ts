import { pushState } from "$app/navigation";
import { page } from "$app/state";
import { MediaQuery } from "svelte/reactivity";
import { PersistedState } from "./storage";

const openSerializer = {
  parse: (saved: string) => saved === "true",
  stringify: (open: boolean) => (open ? "true" : null),
};

/**
 * Whether a right-hand side panel is open. On wide screens it is saved per browser under `key`,
 * so the panel stays open or closed across pages and reloads; the tablet overlay
 * stays closed until it is opened.
 */
export class SidePanelOpen {
  #saved: PersistedState<boolean>;
  #key: string;
  #backPending = false;
  #wide = new MediaQuery("min-width: 1024px");

  constructor(key: string) {
    this.#key = key;
    this.#saved = new PersistedState(key, false, openSerializer);
  }

  /** Whether the panel sits beside the page instead of covering it. */
  get wide(): boolean {
    return this.#wide.current;
  }

  get current(): boolean {
    return this.wide ? this.#saved.current : page.state.sidePanel === this.#key;
  }

  set current(open: boolean) {
    if (this.wide) this.#saved.current = open;
    else if (!this.#backPending && open !== this.current) {
      if (open) pushState("", { ...page.state, sidePanel: this.#key });
      else {
        this.#backPending = true;
        window.addEventListener(
          "popstate",
          () => {
            this.#backPending = false;
          },
          { once: true },
        );
        history.back();
      }
    }
  }
}
