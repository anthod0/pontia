import type {
  DashboardClient,
  ClientControlDetails,
} from "../../apps/dashboard/src/clients/contract";

interface CodexControlDetails extends ClientControlDetails {
  connection: "awaiting_input" | "available" | "reconciling" | "unavailable" | "archived";
  thread_id?: string;
}

export const dashboardClient: DashboardClient = {
  clientType: "codex",
  defaultForCreation: false,
  workspaceCreation: false,
  profileSelection: false,
  archiveNotice: "Codex thread archiving is separate and is not changed here.",
  deliveryPolicy: "steer",
  controlDetails: (session) => (session.codex as CodexControlDetails | undefined) ?? null,
};
