import type { DashboardClient } from "../../apps/dashboard/src/clients/contract";

interface AcpDetails {
  system_prompt_injected?: boolean;
  agent_id?: string;
  name?: string;
  connection?: "connected" | "reconnecting" | "failed" | "exited";
  connection_error?: string | null;
}

const agentNames = new Map([
  ["grok", "Grok Build"],
  ["antigravity", "Antigravity"],
  ["kiro", "Kiro"],
]);

export const dashboardClient: DashboardClient = {
  clientType: "acp",
  displayName: (key, installedName) =>
    key ? (installedName ?? agentNames.get(key) ?? key) : "ACP",
  iconKey: (key) => (key && agentNames.has(key) ? key : undefined),
  requiresClientKey: true,
  sessionKey: (session) => (session.acp as AcpDetails | undefined)?.agent_id,
  instructionsInjected: (session) =>
    (session.acp as AcpDetails | undefined)?.system_prompt_injected,
  defaultForCreation: false,
  controlDetails: (session) => {
    const details = session.acp as AcpDetails | undefined;
    if (!details?.connection) return null;
    return {
      connection: details.connection,
      refreshWhileActive: true,
      diagnostic:
        details.connection === "reconnecting"
          ? { kind: "reconnecting", message: details.connection_error }
          : details.connection === "failed"
            ? { kind: "error", message: details.connection_error }
            : null,
    };
  },
  sessionLabel: (session) => {
    const details = session.acp as AcpDetails | undefined;
    if (!details?.agent_id) return "acp";
    return details.name || details.agent_id;
  },
};
