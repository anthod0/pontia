import { goto } from "$app/navigation";
import { dashboardPath } from "$lib/dashboardRoutes";

export {
  dashboardPath,
  dashboardRelativePath,
  dashboardScope,
  routeParam,
} from "$lib/dashboardRoutes";

export async function navigate(
  path: string,
  query?: Record<string, string | null | undefined>,
  options: { replaceState?: boolean; scope?: string } = {},
): Promise<void> {
  const { scope, ...navigationOptions } = options;
  const target =
    scope === undefined ? dashboardPath(path) : `${scope}${path === "/" ? "" : path}` || "/";
  const url = new URL(target, window.location.origin);
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value != null) url.searchParams.set(key, value);
  }
  await goto(`${url.pathname}${url.search}${url.hash}`, navigationOptions);
  window.dispatchEvent(new PopStateEvent("popstate"));
}
