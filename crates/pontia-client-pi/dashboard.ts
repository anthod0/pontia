import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

export const dashboardClient: DashboardClient = {
  clientType: "pi",
  defaultForCreation: true,
  workspaceCreation: true,
  profileSelection: true,
  deliveryPolicy: "after_idle",
  controlDetails: () => null,
};
