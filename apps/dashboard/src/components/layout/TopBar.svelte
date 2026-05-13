<script lang="ts">
  import { PanelLeft, Search, Settings, TriangleAlert, Wifi, WifiOff } from '@lucide/svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { token } from '../../stores/auth'
  import { lastConnectionError, sseStatus } from '../../stores/connection'

  const statusLabel: Record<string, string> = {
    idle: 'SSE idle',
    connecting: 'SSE connecting',
    open: 'SSE live',
    reconnecting: 'SSE reconnecting',
    closed: 'SSE closed',
    error: 'SSE error',
  }
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

  {#if !$token.trim()}
    <Button variant="destructive" size="sm" class="hidden gap-2 sm:inline-flex" href="/dashboard/settings">
      <TriangleAlert class="size-4" />
      Set API token
    </Button>
  {:else}
    <Badge variant={$sseStatus === 'open' ? 'default' : 'secondary'} class="hidden gap-1 md:inline-flex" title={$lastConnectionError ?? undefined}>
      {#if $sseStatus === 'open'}<Wifi class="size-3" />{:else}<WifiOff class="size-3" />{/if}
      {statusLabel[$sseStatus] ?? $sseStatus}
    </Badge>
  {/if}

  <Button variant="outline" size="sm" class="hidden gap-2 md:inline-flex" href="/dashboard/tasks">
    <Search class="size-4" />
    Browse tasks
  </Button>
  <Button variant="ghost" size="sm" class="gap-2" href="/dashboard/settings">
    <Settings class="size-4" />
    <span class="sr-only sm:not-sr-only">Settings</span>
  </Button>
</header>
