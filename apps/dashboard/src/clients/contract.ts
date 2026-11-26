import type { SessionView } from "../api/types";

export interface ClientControlDetails {
  connection: string;
  refreshWhileActive?: boolean;
  unavailableReason?: string | null;
  diagnostic?: {
    kind: "reconnecting" | "error";
    message?: string | null;
  } | null;
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
