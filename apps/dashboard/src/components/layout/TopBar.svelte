<script lang="ts">
  import { CircleHelp, PanelLeft, Wifi, WifiOff, WifiSync } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { lastConnectionError, sseStatus } from '../../stores/connection'

  let currentPath = $state(window.location.pathname)

  function updatePath(): void {
    currentPath = window.location.pathname
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isChatPath(path: string): boolean {
    return path === '/dashboard/chat' || path.startsWith('/dashboard/chat/')
  }

  function openKeyboardShortcuts(): void {
    document.dispatchEvent(new CustomEvent('pontia:open-chat-shortcuts'))
  }

  const sseTitle = $derived($lastConnectionError ? `SSE ${$sseStatus}: ${$lastConnectionError}` : `SSE ${$sseStatus}`)
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<header class="sticky z-10 flex h-8 items-center bg-surface px-3 md:px-4" style="top: var(--visual-viewport-top, 0px)">
  <Sidebar.Trigger>
    <PanelLeft />
    <span class="sr-only">Toggle sidebar</span>
  </Sidebar.Trigger>
  {#if isChatPath(currentPath)}
    <Button variant="ghost" size="icon-sm" class="ml-1 hidden sm:inline-flex" aria-label="Keyboard shortcuts" onclick={openKeyboardShortcuts}>
      <CircleHelp class="size-4" />
    </Button>
  {/if}
  <span class="ml-auto inline-flex items-center" aria-label={sseTitle} title={sseTitle}>
    {#if $sseStatus === 'open'}
      <Wifi class="size-4" />
    {:else if $sseStatus === 'connecting' || $sseStatus === 'reconnecting'}
      <WifiSync class="size-4" />
    {:else}
      <WifiOff class="size-4" />
    {/if}
  </span>
</header>
