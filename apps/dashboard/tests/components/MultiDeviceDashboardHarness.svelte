<script lang="ts">
  import DashboardModeLayout from '../../src/modes/public/DashboardModeLayout.svelte'
  import DashboardEntryPage from '../../src/pages/DashboardEntryPage.svelte'
  import ChatRouteStub from './ChatRouteStub.svelte'
  let pathname = $state(window.location.pathname)
  const parts = $derived(pathname.split('/').filter(Boolean))
</script>

<svelte:window onpopstate={() => pathname = window.location.pathname} />
<DashboardModeLayout handle={parts[0]}>
  {#if parts.length === 1}<DashboardEntryPage />
  {:else if parts[1] === 'chat'}{#key parts[2]}<ChatRouteStub sessionId={parts[2]} />{/key}
  {:else}<p>Device page</p>{/if}
</DashboardModeLayout>
