import { dashboardClient as pi } from "../../../crates/pontia-client-pi/dashboard";
import { dashboardClient as codex } from "../../../crates/pontia-client-codex/dashboard";
import { dashboardClient as acp } from "../../../crates/pontia-client-acp/dashboard";
import { dashboardClient as claude } from "../../../crates/pontia-client-claude/dashboard";
import type { SessionView } from "./api/types";

const clients = [pi, codex, acp, claude];
const defaultClient = clients.find((client) => client.defaultForCreation);
if (!defaultClient) throw new Error("No default Agent Client is registered");

export const defaultClientType = defaultClient.clientType;

export function dashboardClient(clientType: string | undefined) {
  return clients.find((client) => client.clientType === clientType);
}

export function clientDisplayName(clientType: string): string {
  return dashboardClient(clientType)?.typeLabel ?? clientType;
}

export function sessionClientKey(session: SessionView) {
  return dashboardClient(session.client_type)?.sessionKey?.(session);
}

export function clientSessionLabel(session: SessionView): string {
  return (
    dashboardClient(session.client_type)?.sessionLabel?.(session) ??
    clientDisplayName(session.client_type)
  );
}

export function clientControlDetails(session: SessionView) {
  return dashboardClient(session.client_type)?.controlDetails(session) ?? null;
}

export function clientResumeWaitsForIdle(session: SessionView | null | undefined): boolean {
  return dashboardClient(session?.client_type)?.resumeWaitsForIdle ?? true;
}
