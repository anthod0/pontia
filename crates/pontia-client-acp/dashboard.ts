import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

interface AcpDetails {
  agent_id?: string;
  name?: string;
  connection?: "connected" | "reconnecting" | "failed" | "exited";
  connection_error?: string | null;
}

export const dashboardClient: DashboardClient = {
  clientType: "acp",
  defaultForCreation: false,
  controlDetails: (session) => {
    const details = session.acp as AcpDetails | undefined;
    if (!details?.connection) return null;
    return {
      connection: details.connection,
      refreshWhileActive: true,
      diagnostic:
        details.connection === "reconnecting"
          ? { kind: "reconnecting", message: details.connection_error }
          : details.connection === "failed"
            ? { kind: "error", message: details.connection_error }
            : null,
    };
  },
  sessionLabel: (session) => {
    const details = session.acp as AcpDetails | undefined;
    if (!details?.agent_id) return "acp";
    return details.name || details.agent_id;
  },
};
