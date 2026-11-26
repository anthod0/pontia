import { page } from "$app/state";
import { isMobileShell } from "$lib/hooks/shellMode.svelte";
import { goto } from "$app/navigation";
import { dashboardPath } from "$lib/dashboardRoutes";
import { botPanelTabs } from "$lib/botPanelTab";
import { sessionPanelTabs } from "$lib/sessionPanelTab";
import { sidePanelParam } from "$lib/sidePanelRoute.svelte";

export {
  dashboardPath,
  dashboardRelativePath,
  dashboardScope,
  routeParam,
} from "$lib/dashboardRoutes";

export async function navigate(
  path: string,
  query?: Record<string, string | null | undefined>,
  options: { replaceState?: boolean; scope?: string; preserveSidePanel?: boolean } = {},
): Promise<void> {
  const { scope, preserveSidePanel, ...navigationOptions } = options;
  const target =
    scope === undefined ? dashboardPath(path) : `${scope}${path === "/" ? "" : path}` || "/";
  const url = new URL(target, window.location.origin);
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value != null) url.searchParams.set(key, value);
  }
  const panel = preserveSidePanel
    ? (page.url.searchParams.get(sidePanelParam) ?? page.state.deviceSwitchPanel)
    : null;
  const tabs = path.startsWith("/chat/")
    ? sessionPanelTabs
    : path.startsWith("/bots/")
      ? botPanelTabs
      : [];
  if (panel && tabs.some((tab) => tab === panel)) url.searchParams.set(sidePanelParam, panel);
  const mobileState = isMobileShell()
    ? { mobileBack: !options.replaceState || page.state.mobileBack }
    : undefined;
  const state =
    preserveSidePanel && path === "/" && panel
      ? { ...mobileState, deviceSwitchPanel: panel }
      : mobileState;
  await goto(
    `${url.pathname}${url.search}${url.hash}`,
    state ? { ...navigationOptions, state } : navigationOptions,
  );
  window.dispatchEvent(new PopStateEvent("popstate"));
}
