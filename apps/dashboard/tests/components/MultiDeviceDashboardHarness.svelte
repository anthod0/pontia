<script lang="ts">
  import DashboardModeLayout from '../../src/modes/public/DashboardModeLayout.svelte'
  import NewChatPage from '../../src/pages/NewChatPage.svelte'
  let pathname = $state(window.location.pathname)
  const parts = $derived(pathname.split('/').filter(Boolean))
</script>

<svelte:window onpopstate={() => pathname = window.location.pathname} />
<DashboardModeLayout handle={parts[0]}>
  {#if parts.length === 1}<NewChatPage />{:else}<p>Device page</p>{/if}
</DashboardModeLayout>
