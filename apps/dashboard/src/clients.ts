import { dashboardClient as pi } from "../../../crates/pontia-client-pi/dashboard";
import { dashboardClient as codex } from "../../../crates/pontia-client-codex/dashboard";
import { dashboardClient as acp } from "../../../crates/pontia-client-acp/dashboard";
import { dashboardClient as claude } from "../../../crates/pontia-client-claude/dashboard";
import type { SessionView } from "./api/types";
import { clientDisplayName } from "./lib/clientName";

const clients = [pi, codex, acp, claude];
const defaultClient = clients.find((client) => client.defaultForCreation);
if (!defaultClient) throw new Error("No default Agent Client is registered");

export const defaultClientType = defaultClient.clientType;

export function clientSessionLabel(session: SessionView): string {
  return (
    clients.find((client) => client.clientType === session.client_type)?.sessionLabel?.(session) ??
    clientDisplayName(session.client_type)
  );
}

export function clientControlDetails(session: SessionView) {
  return (
    clients.find((client) => client.clientType === session.client_type)?.controlDetails(session) ??
    null
  );
}

export function clientResumeWaitsForIdle(session: SessionView | null | undefined): boolean {
  return (
    clients.find((client) => client.clientType === session?.client_type)?.resumeWaitsForIdle ?? true
  );
}
