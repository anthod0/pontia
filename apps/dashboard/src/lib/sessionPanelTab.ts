export const sessionPanelTabs = ["files", "preview", "git"] as const;

/** A tab of the Session side panel. */
export type SessionPanelTab = (typeof sessionPanelTabs)[number];
