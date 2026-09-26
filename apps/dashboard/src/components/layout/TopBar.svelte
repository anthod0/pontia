<script lang="ts">
  import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon'
  import SidebarSimpleIcon from 'phosphor-svelte/lib/SidebarSimpleIcon'
  import WifiHighIcon from 'phosphor-svelte/lib/WifiHighIcon'
  import WifiSlashIcon from 'phosphor-svelte/lib/WifiSlashIcon'
  import WifiMediumIcon from 'phosphor-svelte/lib/WifiMediumIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { lastConnectionError, sseStatus } from '../../stores/connection'
  import { sessionDetail, sessionDetailError, sessions } from '../../stores/sessions'

  let currentPath = $state(window.location.pathname)

  function updatePath(): void {
    currentPath = window.location.pathname
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/dashboard' || path === '/dashboard/' || path.startsWith('/dashboard/chat/')
  }

  function openKeyboardShortcuts(): void {
    document.dispatchEvent(new CustomEvent('pontia:open-chat-shortcuts'))
  }

  const sseTitle = $derived($lastConnectionError ? `SSE ${$sseStatus}: ${$lastConnectionError}` : `SSE ${$sseStatus}`)
  const sessionId = $derived(currentPath.startsWith('/dashboard/chat/') ? decodeURIComponent(currentPath.split('/')[3] ?? '') : '')
  const session = $derived(sessionId
    ? ($sessionDetail?.session.session_id === sessionId ? $sessionDetail.session : ($sessionDetailError ? null : $sessions.find((item) => item.session_id === sessionId) ?? null))
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
