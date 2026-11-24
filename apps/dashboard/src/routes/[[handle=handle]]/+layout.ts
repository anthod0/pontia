import { validateDashboardRoute } from "$dashboard-mode/routePolicy";

export function load({
  params,
  route,
}: {
  params: Record<string, string | undefined>;
  route: { id: string | null };
}) {
  const handle = validateDashboardRoute({ handle: params.handle, routeId: route.id });
  return { handle, ...(handle && !params.handle ? { deviceEntry: true } : {}) };
}
