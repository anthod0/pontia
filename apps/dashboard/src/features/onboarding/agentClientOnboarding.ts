import type { AgentClientAuthProbe, AgentClientAuthStatus, AgentClientView } from "../../api/types";

export const agentClientOnboardingMarkerPrefix = "pontia.agentClientOnboarding.checked.";

export function agentClientOnboardingMarker(deviceId: string): string {
  return `${agentClientOnboardingMarkerPrefix}${deviceId}`;
}

export function authForClient(
  client: AgentClientView,
  probe: AgentClientAuthProbe,
): AgentClientAuthStatus | undefined {
  return probe.agent_clients.find(
    (entry) => entry.client_type === client.client_type && entry.client_key === client.client_key,
  )?.auth;
}

export function shouldShowAgentClientOnboarding(
  clients: AgentClientView[],
  probe: AgentClientAuthProbe,
): boolean {
  if (!probe.first_probe) return false;
  const installed = clients.filter((client) => client.detection.status !== "not_detected");
  if (installed.length === 0) return true;
  if (!installed.some((client) => client.enabled)) return true;
  return !installed.some((client) => authForClient(client, probe)?.state === "authenticated");
}

export async function runAgentClientOnboardingProbe<T>({
  deviceId,
  storage,
  probe,
  now = () => new Date().toISOString(),
}: {
  deviceId: string;
  storage: Pick<Storage, "getItem" | "setItem">;
  probe: () => Promise<T>;
  now?: () => string;
}): Promise<{ checked: boolean; result?: T }> {
  const marker = agentClientOnboardingMarker(deviceId);
  if (storage.getItem(marker) !== null) return { checked: false };
  storage.setItem(marker, now());
  return { checked: true, result: await probe() };
}

export type OnboardingReason = "no_installed" | "no_enabled" | "no_signed_in" | "unknown";

export function onboardingReason(
  clients: AgentClientView[],
  auth: ReadonlyMap<string, AgentClientAuthStatus>,
): OnboardingReason {
  const installed = clients.filter((client) => client.detection.status !== "not_detected");
  if (installed.length === 0) return "no_installed";
  const usable = installed.filter((client) => client.enabled && client.setup.status !== "required");
  if (
    usable.length > 0 &&
    usable.every((client) => {
      const state = auth.get(client.id)?.state;
      return state === "unknown" || state === "unsupported" || state === undefined;
    })
  )
    return "unknown";
  if (!installed.some((client) => auth.get(client.id)?.state === "authenticated"))
    return "no_signed_in";
  return "no_enabled";
}
