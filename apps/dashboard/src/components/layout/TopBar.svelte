<script lang="ts">
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionDetail, sessionDetailError } from '../../stores/sessions'
  import { createSessionOverviewQuery, snapshotSessionOverview } from '../../queries/sessionOverview'

  let currentPath = $state(dashboardRelativePath())
  const overviewQuery = createSessionOverviewQuery()
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))

  function updatePath(): void {
    currentPath = dashboardRelativePath()
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/' || path.startsWith('/chat/')
  }

  const sessionId = $derived(currentPath.startsWith('/chat/') ? decodeURIComponent(currentPath.split('/')[2] ?? '') : '')
  const overviewSession = $derived(
    overview.pinned.find((item) => item.session_id === sessionId)
      ?? overview.active.find((item) => item.session_id === sessionId)
      ?? overview.list.find((item) => item.session_id === sessionId)
      ?? null
  )
  const session = $derived(sessionId
    ? ($sessionDetail?.session.session_id === sessionId ? $sessionDetail.session : ($sessionDetailError ? null : overviewSession))
    : null)
  const title = $derived(session ? sessionChatTitle(session) : '')
  const chatPath = $derived(isChatPath(currentPath))
  const groupClass = $derived(['flex min-w-0 items-center gap-1', chatPath && 'pointer-events-auto h-8 rounded-lg bg-popover px-1 ring-1 ring-foreground/10', chatPath && !title && 'md:hidden'])
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<header class={['sticky z-10 flex items-center px-3 md:px-4', chatPath ? 'pointer-events-none h-12' : 'h-8 bg-surface']} style="top: var(--visual-viewport-top, 0px)">
  <div class={groupClass}>
    <Sidebar.Trigger class="md:hidden" />
    {#if title}
      <h1 class="mx-3 min-w-0 max-w-80 truncate text-base font-normal text-heading" title={title}>{title}</h1>
    {/if}
  </div>
</header>
