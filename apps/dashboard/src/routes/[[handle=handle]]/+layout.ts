import { validateDashboardRoute } from "$dashboard-mode/routePolicy";

export function load({
  params,
  route,
}: {
  params: Record<string, string | undefined>;
  route: { id: string | null };
}) {
  const handle = params.handle;
  validateDashboardRoute({ handle, routeId: route.id });
  return { handle };
}
