import type { InboxDeliveryPolicy, SessionView } from "../api/types";

export interface ClientControlDetails {
  connection: string;
  profile?: {
    profile_id?: string;
    version?: string;
    status: string;
    error?: string;
  } | null;
}

export interface DashboardClient {
  clientType: string;
  defaultForCreation: boolean;
  workspaceCreation: boolean;
  profileSelection: boolean;
  deliveryPolicy: InboxDeliveryPolicy;
  controlDetails(session: SessionView): ClientControlDetails | null;
}
