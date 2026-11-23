import { writable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

export type SidebarSessionScope = "active" | "all";

// Per device; chat shortcuts follow the workspace filter so they match the sidebar's active sessions.
export function createSidebarSessionFilter(_device: DeviceRuntime) {
  const selectedWorkspaceId = writable("");
  const sessionScope = writable<SidebarSessionScope>("active");
  return { selectedWorkspaceId, sessionScope };
}
