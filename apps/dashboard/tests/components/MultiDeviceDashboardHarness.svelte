<script lang="ts">
  import DashboardModeLayout from '../../src/modes/public/DashboardModeLayout.svelte'
  import DashboardEntryPage from '../../src/pages/DashboardEntryPage.svelte'
  import { isValidDeviceHandle } from '../../src/lib/deviceHandle'
  import ShortcutSettings from '../../src/components/settings/ShortcutSettings.svelte'
  import ChatRouteStub from './ChatRouteStub.svelte'
  let pathname = $state(window.location.pathname)
  const parts = $derived(pathname.split('/').filter(Boolean))
</script>

<svelte:window onpopstate={() => pathname = window.location.pathname} />
<DashboardModeLayout handle={parts[0] && isValidDeviceHandle(parts[0]) ? parts[0] : undefined}>
  {#if pathname.endsWith('/settings/shortcuts')}<ShortcutSettings />
  {:else if parts.length === 1}<DashboardEntryPage />
  {:else if parts[1] === 'chat'}{#key parts[2]}<ChatRouteStub sessionId={parts[2]} />{/key}
  {:else}<p>Device page</p>{/if}
</DashboardModeLayout>
