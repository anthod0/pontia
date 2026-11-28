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
  displayName(key?: string, installedName?: string): string;
  iconKey(key?: string): string | undefined;
  typeLabel?: string;
  titleLabel?: string;
  requiresClientKey?: boolean;
  sessionKey?(session: SessionView): string | undefined;
  instructionsInjected?(session: SessionView): boolean | undefined;
  defaultForCreation: boolean;
  resumeWaitsForIdle?: boolean;
  controlDetails(session: SessionView): ClientControlDetails | null;
  sessionLabel?(session: SessionView): string;
}
