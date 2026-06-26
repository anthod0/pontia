<script lang="ts">
  import { CircleHelp, PanelLeft } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'

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
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<header class="sticky top-0 z-10 flex h-10 items-center bg-transparent px-3 md:px-4">
  <Sidebar.Trigger>
    <PanelLeft />
    <span class="sr-only">Toggle sidebar</span>
  </Sidebar.Trigger>
  {#if isChatPath(currentPath)}
    <Button variant="ghost" size="icon-sm" class="ml-1 hidden sm:inline-flex" aria-label="Keyboard shortcuts" onclick={openKeyboardShortcuts}>
      <CircleHelp class="size-4" />
    </Button>
  {/if}
</header>
