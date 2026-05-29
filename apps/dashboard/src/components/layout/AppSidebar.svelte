<script lang="ts">
  import { GitBranch, Home, SquarePen } from '@lucide/svelte'
  import { navigate } from 'svelte-mini-router'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import { sessions, sessionsLoading } from '../../stores/sessions'
  import { sessionChatTitle, visibleChatSessions } from '$lib/session-chat/sessionChat'

  type Item = {
    label: string
    path: string
    icon: typeof Home
  }

  const primaryItems: Item[] = [
    { label: 'Overview', path: '/overview', icon: Home },
    { label: 'Tasks', path: '/tasks', icon: GitBranch },
    { label: 'New Chat', path: '/chat', icon: SquarePen },
  ]

  const recentSessionLimit = 8

  let currentPath = $state(normalizePath(window.location.pathname))
  let recentSessions = $derived(visibleChatSessions($sessions, 'all').slice(0, recentSessionLimit))

  function normalizePath(pathname: string) {
    return pathname.replace(/^\/dashboard/, '') || '/'
  }

  function isActive(path: string) {
    if (path === '/tasks') return currentPath === '/tasks' || currentPath.startsWith('/tasks/')
    if (path === '/chat') return currentPath === '/chat' || currentPath.startsWith('/chat/')
    return currentPath === path
  }

  function activeSessionIdFromPath(): string | null {
    const match = currentPath.match(/^\/(?:chat|sessions)\/([^/?#]+)/)
    return match ? decodeURIComponent(match[1]) : null
  }

  function isSessionActive(sessionId: string) {
    return activeSessionIdFromPath() === sessionId
  }

  function isSessionActiveState(state: string) {
    return state !== 'exited' && state !== 'error'
  }

  function go(path: string) {
    navigate(path)
    currentPath = normalizePath(path)
  }

  function openSession(sessionId: string) {
    navigate(`/chat/${sessionId}`)
    currentPath = `/chat/${sessionId}`
    window.dispatchEvent(new PopStateEvent('popstate'))
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

    <Sidebar.Group>
      <Sidebar.GroupLabel>Recent Sessions</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#if $sessionsLoading && !recentSessions.length}
            <Sidebar.MenuSkeleton />
            <Sidebar.MenuSkeleton />
          {:else if recentSessions.length}
            {#each recentSessions as session}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session)} · ${session.state}`} onclick={() => openSession(session.session_id)}>
                  <span class="line-clamp-1">{sessionChatTitle(session)}</span>
                  {#if isSessionActiveState(session.state)}
                    <span class="ml-auto size-2 shrink-0 rounded-full bg-green-500 group-data-[collapsible=icon]:hidden" aria-label="Active session"></span>
                  {/if}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/each}
          {:else}
            <Sidebar.MenuItem>
              <div class="px-2 py-1 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">No active sessions</div>
            </Sidebar.MenuItem>
          {/if}
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>
  </Sidebar.Content>
  <Sidebar.Rail />
</Sidebar.Root>
