<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'
  import ChatShortcuts from '../chat/ChatShortcuts.svelte'
  import SettingsShell from '../settings/SettingsShell.svelte'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { installVisualViewportCssVars } from '$lib/visualViewport'

  let { children }: { children: Snippet } = $props()
  let currentPath = $state(dashboardRelativePath())

  function updatePath(): void {
    currentPath = dashboardRelativePath()
  }

  function updatePathAfterNavigation(): void {
    setTimeout(updatePath, 0)
  }

  function isSettingsPath(path: string): boolean {
    return path === '/settings' || path.startsWith('/settings/')
  }

  function isChatPath(path: string): boolean {
    return path === '/' || path.startsWith('/chat/')
  }

  const settingsPath = $derived(isSettingsPath(currentPath))
  const chatPath = $derived(isChatPath(currentPath))
  const mainClass = $derived(settingsPath ? 'min-w-0 flex-1 bg-surface' : chatPath ? 'min-w-0 flex-1 bg-surface px-4 md:px-8' : 'min-w-0 flex-1 bg-surface p-4 md:p-6')

  let uninstallVisualViewportCssVars: (() => void) | null = null

  onMount(() => {
    uninstallVisualViewportCssVars = installVisualViewportCssVars()
  })

  onDestroy(() => {
    uninstallVisualViewportCssVars?.()
  })
  const contentClass = $derived(settingsPath || chatPath ? 'min-w-0 w-full' : 'mx-auto min-w-0 w-full max-w-7xl')
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
