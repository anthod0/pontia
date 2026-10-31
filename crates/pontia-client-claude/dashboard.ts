import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

export const dashboardClient: DashboardClient = {
  clientType: "claude-agent",
  defaultForCreation: false,
  workspaceCreation: true,
  profileSelection: false,
  deliveryPolicy: "after_idle",
  resumeWaitsForIdle: false,
  controlDetails: (session) => {
    const reason = (session["claude-agent"] as { unavailable_reason?: string | null } | undefined)
      ?.unavailable_reason;
    return {
      connection: session.state === "exited" ? "exited" : reason ? "unavailable" : "connected",
      refreshWhileActive: true,
      unavailableReason: reason
        ? reason === "authentication_failed_retrying"
          ? "Claude authentication failed. The SDK is retrying; check your Claude Code authentication."
          : reason === "api_request_retrying"
            ? "Claude's model request failed. The SDK is retrying."
            : `Claude Agent control unavailable: ${reason.replaceAll("_", " ")}`
        : null,
    };
  },
  sessionLabel: () => "Claude Agent",
};
