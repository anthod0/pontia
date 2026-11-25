export const botPanelTabs = ["files", "preview", "context", "todo", "search", "git"] as const;

/** A tab of the Bot chat side panel. */
export type PanelTab = (typeof botPanelTabs)[number];
