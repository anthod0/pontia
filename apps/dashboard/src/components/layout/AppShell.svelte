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
</script>

<svelte:window onpopstate={updatePath} onclick={updatePathAfterNavigation} />

<Sidebar.Provider>
  <AppSidebar />
  <Sidebar.Inset>
    <TopBar />
    <main class="flex-1 bg-muted/20 p-4 md:p-6">
      <div class="mx-auto w-full max-w-7xl">
        {#if isSettingsPath(currentPath)}
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
