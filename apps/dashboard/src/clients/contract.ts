import type { SessionView } from "../api/types";

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
  defaultForCreation: boolean;
  resumeWaitsForIdle?: boolean;
  controlDetails(session: SessionView): ClientControlDetails | null;
  sessionLabel?(session: SessionView): string;
}
