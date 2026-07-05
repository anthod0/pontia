<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'
  import ChatShortcuts from '../chat/ChatShortcuts.svelte'
  import SettingsShell from '../settings/SettingsShell.svelte'
  import { installVisualViewportCssVars } from '$lib/visualViewport'

  let { children }: { children: Snippet } = $props()
  let currentPath = $state(window.location.pathname)

  function updatePath(): void {
    currentPath = window.location.pathname
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isSettingsPath(path: string): boolean {
    return path === '/dashboard/settings' || path.startsWith('/dashboard/settings/')
  }

  function isChatPath(path: string): boolean {
    return path === '/dashboard/chat' || path.startsWith('/dashboard/chat/')
  }

  function isChatSessionPath(path: string): boolean {
    return path.startsWith('/dashboard/chat/')
  }

  const settingsPath = $derived(isSettingsPath(currentPath))
  const chatPath = $derived(isChatPath(currentPath))
  const chatSessionPath = $derived(isChatSessionPath(currentPath))
  const mainClass = $derived(settingsPath ? 'min-w-0 flex-1 bg-surface' : chatSessionPath ? 'min-w-0 flex-1 bg-surface p-4 pb-40 md:p-6 md:pb-44' : chatPath ? 'min-w-0 flex-1 bg-surface p-4 md:p-6' : 'min-w-0 flex-1 bg-surface p-4 md:p-6')

  let uninstallVisualViewportCssVars: (() => void) | null = null

  onMount(() => {
    uninstallVisualViewportCssVars = installVisualViewportCssVars()
  })

  onDestroy(() => {
    uninstallVisualViewportCssVars?.()
  })
  const contentClass = $derived(settingsPath ? 'min-w-0 w-full' : 'mx-auto min-w-0 w-full max-w-7xl')
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider>
  <ChatShortcuts />
  <AppSidebar />
  <Sidebar.Inset class="bg-surface">
    <TopBar />
    <main class={mainClass}>
      <div class={contentClass}>
        {#if settingsPath}
          <SettingsShell>
            {@render children()}
          </SettingsShell>
        {:else}
          {@render children()}
        {/if}
      </div>
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>
