<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import AppSidebar from './AppSidebar.svelte'
  import TopBar from './TopBar.svelte'

  let { children }: { children: Snippet } = $props()
</script>

<Sidebar.Provider>
  <AppSidebar />
  <Sidebar.Inset>
    <TopBar />
    <main class="flex-1 bg-muted/20 p-4 md:p-6">
      <div class="mx-auto w-full max-w-7xl">
        {@render children()}
      </div>
    </main>
  </Sidebar.Inset>
</Sidebar.Provider>
