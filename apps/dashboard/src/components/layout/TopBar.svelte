<script lang="ts">
  import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon'
  import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon'
  import WifiHighIcon from 'phosphor-svelte/lib/WifiHighIcon'
  import WifiSlashIcon from 'phosphor-svelte/lib/WifiSlashIcon'
  import WifiMediumIcon from 'phosphor-svelte/lib/WifiMediumIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { lastConnectionError, sseStatus } from '../../stores/connection'
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

  function openKeyboardShortcuts(): void {
    document.dispatchEvent(new CustomEvent('pontia:open-chat-shortcuts'))
  }

  const sseTitle = $derived($lastConnectionError ? `SSE ${$sseStatus}: ${$lastConnectionError}` : `SSE ${$sseStatus}`)
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
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<header class="sticky z-10 flex h-8 items-center bg-surface px-3 md:px-4" style="top: var(--visual-viewport-top, 0px)">
  <Sidebar.Trigger>
    <SidebarSimpleIcon />
    <span class="sr-only">Toggle sidebar</span>
  </Sidebar.Trigger>
  {#if isChatPath(currentPath)}
    <Button variant="ghost" size="icon-sm" class="ml-1 hidden sm:inline-flex" aria-label="Keyboard shortcuts" onclick={openKeyboardShortcuts}>
      <QuestionIcon class="size-4" />
    </Button>
  {/if}
  {#if title}
    <h1 class="mx-3 min-w-0 truncate text-base font-normal text-heading" title={title}>{title}</h1>
  {/if}
  <span class="ml-auto inline-flex shrink-0 items-center" aria-label={sseTitle} title={sseTitle}>
    {#if $sseStatus === 'open'}
      <WifiHighIcon class="size-4 text-aqua" />
    {:else if $sseStatus === 'connecting' || $sseStatus === 'reconnecting'}
      <WifiMediumIcon class="size-4 animate-pulse text-warning" />
    {:else}
      <WifiSlashIcon class="size-4 text-destructive" />
    {/if}
  </span>
</header>
