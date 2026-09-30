import { error } from "@sveltejs/kit";

const rootRouteId = "/[[handle=handle]]";

export function validateDashboardRoute({
  handle,
  routeId,
}: {
  handle?: string;
  routeId: string | null;
}): void {
  if (!handle && routeId !== rootRouteId) {
    error(404, "Not found");
  }
}
