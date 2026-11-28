import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

export const dashboardClient: DashboardClient = {
  clientType: "pi",
  displayName: () => "Pi",
  iconKey: () => "pi",
  typeLabel: "Pi",
  defaultForCreation: true,
  controlDetails: () => null,
};
