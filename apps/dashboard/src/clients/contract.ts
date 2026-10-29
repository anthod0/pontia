import type { InboxDeliveryPolicy, SessionView } from "../api/types";

export interface ClientControlDetails {
  connection: string;
  refreshWhileActive?: boolean;
  unavailableReason?: string | null;
  profile?: {
    profile_id?: string;
    version?: string;
    status: string;
    error?: string;
  } | null;
}

export interface DashboardClient {
  clientType: string;
  customEntry?: { label: string; keyLabel: string; validateKey(key: string): string | null };
  defaultForCreation: boolean;
  workspaceCreation: boolean;
  profileSelection: boolean;
  deliveryPolicy: InboxDeliveryPolicy;
  controlDetails(session: SessionView): ClientControlDetails | null;
  sessionLabel?(session: SessionView): string;
}
