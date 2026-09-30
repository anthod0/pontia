import { isValidDeviceHandle } from "$lib/remoteDashboard";

export function dashboardScope(pathname: string): string {
  const handle = pathname.split("/").filter(Boolean)[0] ?? "";
  return isValidDeviceHandle(handle) ? `/${handle}` : "";
}
