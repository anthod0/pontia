import { dashboardScope as resolveDashboardScope } from "$dashboard-mode/dashboardScope";

export function dashboardScope(pathname = window.location.pathname): string {
  return resolveDashboardScope(pathname);
}

export function dashboardPath(path: string, pathname = window.location.pathname): string {
  if (!path.startsWith("/")) throw new Error(`Dashboard paths must be absolute: ${path}`);
  const scope = dashboardScope(pathname);
  return path === "/" ? scope || "/" : `${scope}${path}`;
}

export function dashboardRelativePath(pathname = window.location.pathname): string {
  const scope = dashboardScope(pathname);
  return scope && pathname.startsWith(scope) ? pathname.slice(scope.length) || "/" : pathname;
}

export function routeParam(segment: string, pathname = dashboardRelativePath()): string | null {
  const parts = pathname.split("/").filter(Boolean);
  const index = parts.indexOf(segment);
  return index >= 0 && index + 1 < parts.length ? decodeURIComponent(parts[index + 1]) : null;
}
