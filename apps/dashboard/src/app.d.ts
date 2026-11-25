declare namespace App {
  interface PageState {
    profileDraft?: import("./pages/agentProfiles/form").AgentProfileDraft;
    /** Set on the history entry the side panel overlay added. */
    sidePanel?: boolean;
    mobileBack?: boolean;
    mobileLayer?: string;
  }
}
