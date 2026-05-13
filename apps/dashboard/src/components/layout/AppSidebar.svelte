<script lang="ts">
  import { Activity, Bot, Boxes, GitBranch, Home, Settings, Wrench } from '@lucide/svelte'
  import { navigate } from 'svelte-mini-router'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'

  type Item = {
    label: string
    path: string
    icon: typeof Home
  }

  const primaryItems: Item[] = [
    { label: 'Overview', path: '/overview', icon: Home },
    { label: 'DAG Tasks', path: '/tasks', icon: GitBranch },
    { label: 'Workspaces', path: '/workspaces', icon: Boxes },
    { label: 'Agent Profiles', path: '/agent-profiles', icon: Bot },
    { label: 'Settings', path: '/settings', icon: Settings },
  ]

  const diagnosticsItems: Item[] = [
    { label: 'Task Sessions', path: '/tasks/example/sessions', icon: Activity },
    { label: 'Artifacts', path: '/tasks/example/artifacts', icon: Wrench },
  ]

  let currentPath = $state(normalizePath(window.location.pathname))

  function normalizePath(pathname: string) {
    return pathname.replace(/^\/dashboard/, '') || '/'
  }

  function isActive(path: string) {
    if (path === '/tasks') return currentPath === '/tasks' || currentPath.startsWith('/tasks/')
    return currentPath === path
  }

  function go(path: string) {
    navigate(path)
    currentPath = path
  }
</script>

<svelte:window onpopstate={() => (currentPath = normalizePath(window.location.pathname))} />

<Sidebar.Root collapsible="icon">
  <Sidebar.Header>
    <button
      type="button"
      class="flex items-center gap-2 rounded-md px-2 py-2 text-left text-sm font-semibold hover:bg-sidebar-accent"
      onclick={() => go('/overview')}
      aria-label="Open dashboard overview"
    >
      <span class="flex size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">ll</span>
      <span class="truncate">llmparty Dashboard</span>
    </button>
  </Sidebar.Header>
  <Sidebar.Content>
    <Sidebar.Group>
      <Sidebar.GroupLabel>Workflow</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#each primaryItems as item}
            <Sidebar.MenuItem>
              <Sidebar.MenuButton isActive={isActive(item.path)} tooltipContent={item.label} onclick={() => go(item.path)}>
                <item.icon />
                <span>{item.label}</span>
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          {/each}
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>

    <Sidebar.Separator />

    <Sidebar.Group>
      <Sidebar.GroupLabel>Diagnostics</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#each diagnosticsItems as item}
            <Sidebar.MenuItem>
              <Sidebar.MenuButton isActive={isActive(item.path)} tooltipContent={item.label} onclick={() => go(item.path)}>
                <item.icon />
                <span>{item.label}</span>
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          {/each}
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>
  </Sidebar.Content>
  <Sidebar.Footer>
    <div class="px-2 py-1 text-xs text-muted-foreground">External API only</div>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>
