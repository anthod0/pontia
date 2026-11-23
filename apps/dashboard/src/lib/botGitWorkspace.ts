import { PersistedState, scopedKey, textSerializer } from "./storage";

// The Workspace a Bot chat's Git tab shows survives reloads, per device and Bot.
export function createGitWorkspace(deviceId: string, botId: string): PersistedState<string | null> {
  return new PersistedState(
    scopedKey("pontia.botGitWorkspace", deviceId, botId),
    null,
    textSerializer,
  );
}
