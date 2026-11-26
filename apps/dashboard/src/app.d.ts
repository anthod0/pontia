declare namespace App {
  interface PageState {
    profileDraft?: import("./pages/agentProfiles/form").AgentProfileDraft;
    /** Set on the history entry the side panel overlay added. */
    sidePanel?: boolean;
    /** Carries a side panel tab while a device switch restores the target device's last page. */
    deviceSwitchPanel?: string;
    mobileBack?: boolean;
    mobileLayer?: string;
  }
}
