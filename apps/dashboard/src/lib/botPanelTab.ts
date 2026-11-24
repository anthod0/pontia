import { PersistedState } from "./storage";

const panelTabs = ["files", "preview", "context", "todo", "search", "git"] as const;

/** A tab of the Bot chat side panel. */
export type PanelTab = (typeof panelTabs)[number];

// The side panel reopens on the tab the user last had open, in every Bot chat on this browser.
export function createPanelTab(): PersistedState<PanelTab> {
  return new PersistedState<PanelTab>("pontia.botPanelTab", "files", {
    parse: (saved) => panelTabs.find((tab) => tab === saved),
    stringify: (tab) => tab,
  });
}
