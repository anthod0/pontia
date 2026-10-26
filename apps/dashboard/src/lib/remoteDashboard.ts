import { isValidDeviceHandle } from "./deviceHandle";

export type RemoteDashboardState =
  | "connecting"
  | "available"
  | "authorization-required"
  | "unavailable"
  | "invalid";

export function initialRemoteDashboardState(handle: string): RemoteDashboardState {
  return isValidDeviceHandle(handle) ? "connecting" : "invalid";
}
