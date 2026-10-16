import { dashboardClient as pi } from "../../../crates/pontia-client-pi/dashboard";
import { dashboardClient as codex } from "../../../crates/pontia-client-codex/dashboard";
import { dashboardClient as acp } from "../../../crates/pontia-client-acp/dashboard";
import type { SessionView } from "./api/types";

const clients = [pi, codex, acp];
const defaultClient = clients.find((client) => client.defaultForCreation);
if (!defaultClient) throw new Error("No default Agent Client is registered");

export const defaultClientType = defaultClient.clientType;
export const workspaceClientTypes = clients
  .filter((client) => client.workspaceCreation)
  .map((client) => client.clientType);
export const profileClientTypes = clients
  .filter((client) => client.profileSelection)
  .map((client) => client.clientType);

export function clientSessionLabel(session: SessionView): string {
  return (
    clients.find((client) => client.clientType === session.client_type)?.sessionLabel?.(session) ??
    session.client_type
  );
}

export function clientControlDetails(session: SessionView) {
  return (
    clients.find((client) => client.clientType === session.client_type)?.controlDetails(session) ??
    null
  );
}

export function clientDeliveryPolicy(session: SessionView | null | undefined) {
  return (
    clients.find((client) => client.clientType === session?.client_type)?.deliveryPolicy ??
    "after_idle"
  );
}
