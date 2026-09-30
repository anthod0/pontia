export type RemoteDashboardState =
  | "connecting"
  | "available"
  | "authorization-required"
  | "unavailable"
  | "invalid";

const deviceHandle = /^[a-z][a-z0-9_-]{3,47}$/;
const reservedHandles = new Set([
  "agent-profiles",
  "api",
  "assets",
  "auth",
  "chat",
  "devices",
  "login",
  "sessions",
  "settings",
  "workflow",
  "workflows",
  "workspace",
  "workspaces",
]);

export function isValidDeviceHandle(value: string): boolean {
  return deviceHandle.test(value) && !reservedHandles.has(value);
}

export function initialRemoteDashboardState(handle: string): RemoteDashboardState {
  return isValidDeviceHandle(handle) ? "connecting" : "invalid";
}
