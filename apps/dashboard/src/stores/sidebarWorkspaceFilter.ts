import { writable } from "svelte/store";
import type { DeviceRuntime } from "../device/runtime";

// Shared so chat shortcuts follow the same Recent Sessions list the sidebar shows.
export function createSidebarWorkspaceFilter(_device: DeviceRuntime) {
  const selectedWorkspaceId = writable("");
  return { selectedWorkspaceId };
}
