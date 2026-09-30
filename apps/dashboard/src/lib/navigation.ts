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
  options: { replaceState?: boolean } = {},
): Promise<void> {
  const url = new URL(dashboardPath(path), window.location.origin);
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value != null) url.searchParams.set(key, value);
  }
  await goto(`${url.pathname}${url.search}${url.hash}`, options);
  window.dispatchEvent(new PopStateEvent("popstate"));
}
