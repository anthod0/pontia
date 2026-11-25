import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

export const dashboardClient: DashboardClient = {
  clientType: "acp",
  defaultForCreation: false,
  controlDetails: () => null,
  sessionLabel: (session) => {
    const details = session.acp as { agent_id?: string; name?: string } | undefined;
    if (!details?.agent_id) return "acp";
    return details.name || details.agent_id;
  },
};
