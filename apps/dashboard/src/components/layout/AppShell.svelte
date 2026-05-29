<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'
  import SettingsShell from '../settings/SettingsShell.svelte'

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

  const settingsPath = $derived(isSettingsPath(currentPath))
  const chatPath = $derived(isChatPath(currentPath))
  const mainClass = $derived(settingsPath ? 'flex-1 bg-muted/20' : chatPath ? 'min-h-0 flex-1 overflow-hidden bg-muted/20 p-4 md:p-6' : 'flex-1 bg-muted/20 p-4 md:p-6')
  const contentClass = $derived(settingsPath ? 'w-full' : chatPath ? 'mx-auto h-full min-h-0 w-full max-w-7xl' : 'mx-auto w-full max-w-7xl')
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider>
  <AppSidebar />
  <Sidebar.Inset class={chatPath ? 'h-svh min-h-0 overflow-hidden' : undefined}>
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
