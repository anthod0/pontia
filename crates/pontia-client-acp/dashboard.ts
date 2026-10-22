import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

export const dashboardClient: DashboardClient = {
  clientType: "acp",
  customEntry: { label: "Add custom client", keyLabel: "ACP Agent ID" },
  defaultForCreation: false,
  workspaceCreation: false,
  profileSelection: false,
  deliveryPolicy: "after_idle",
  controlDetails: () => null,
  sessionLabel: (session) => {
    const details = session.acp as
      | {
          agent_id?: string;
          name?: string;
          agent_info?: { name: string; title?: string | null; version: string };
        }
      | undefined;
    if (!details?.agent_id) return "acp";
    const selected =
      details.name && details.name !== details.agent_id
        ? `${details.name} (${details.agent_id})`
        : details.agent_id;
    const info = details.agent_info;
    return info ? `${selected} · ${info.title ?? info.name} ${info.version}` : selected;
  },
};
