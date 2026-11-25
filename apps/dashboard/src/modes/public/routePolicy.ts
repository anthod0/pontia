import { error } from "@sveltejs/kit";

const rootRouteId = "/[[handle=handle]]";

export function validateDashboardRoute({
  handle,
  routeId,
}: {
  handle?: string;
  routeId: string | null;
}): string | undefined {
  // These bare paths already identify devices with these handles in public mode.
  if (!handle && routeId === rootRouteId + "/bots") return "bots";
  if (!handle && routeId === rootRouteId + "/todos") return "todos";
  if (
    !handle &&
    routeId !== rootRouteId &&
    routeId !== `${rootRouteId}/bot` &&
    routeId !== `${rootRouteId}/settings/shortcuts`
  ) {
    error(404, "Not found");
  }
  return handle;
}
