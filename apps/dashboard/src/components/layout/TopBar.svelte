<script lang="ts">
  import { PanelLeft, Search } from '@lucide/svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
</script>

<header class="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur md:px-6">
  <Sidebar.Trigger>
    <PanelLeft />
    <span class="sr-only">Toggle sidebar</span>
  </Sidebar.Trigger>
  <div class="min-w-0 flex-1">
    <h1 class="truncate text-sm font-medium">Dashboard v2</h1>
    <p class="hidden text-xs text-muted-foreground sm:block">DAG tasks, workspaces, profiles, and execution diagnostics</p>
  </div>
  <Button variant="outline" size="sm" class="hidden gap-2 md:inline-flex" href="/dashboard/tasks">
    <Search class="size-4" />
    Browse tasks
  </Button>
</header>
