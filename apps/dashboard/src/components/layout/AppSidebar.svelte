<script lang="ts">
  import { ChevronDown, Folder, Pencil, Settings, SquarePen } from '@lucide/svelte'
  import { navigate } from 'svelte-mini-router'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { sidebarMenuButtonVariants } from '$lib/components/ui/sidebar/sidebar-menu-button.svelte'
  import { cn } from '$lib/utils.js'
  import { sessions, sessionsLoading, updateSessionTitle } from '../../stores/sessions'
  import { workspaces, workspacesLoading } from '../../stores/workspaces'
  import { sessionChatTitle, visibleChatSessions } from '$lib/session-chat/sessionChat'
  import { workspaceTitle } from '../chat/sessionMetadata'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import type { SessionView, WorkspaceView } from '../../api/types'

  type Item = {
    label: string
    path: string
    icon: typeof SquarePen
  }

  type RecentWorkspaceGroup = {
    workspace: WorkspaceView
    sessions: SessionView[]
  }

  const primaryItems: Item[] = [
    { label: 'New Chat', path: '/chat', icon: SquarePen },
  ]

  const recentSessionLimit = 50

  const settingsSections = [
    { label: 'Common', path: '/settings/common' },
    { label: 'Workspaces', path: '/settings/workspaces' },
    { label: 'Agent Profiles', path: '/settings/agent-profiles' },
  ]

  let currentPath = $state(normalizePath(window.location.pathname))
  let renamingSessionId = $state<string | null>(null)
  let renamingSession = $state<SessionView | null>(null)
  let renameDialogOpen = $state(false)
  let renameError = $state<string | null>(null)
  let recentSessionsOpen = $state(true)
  let expandedWorkspaceIds = $state<Record<string, boolean>>({})
  let recentSessions = $derived(visibleChatSessions($sessions, 'all').slice(0, recentSessionLimit))
  let activeWorkspaces = $derived($workspaces.filter((workspace) => workspace.state === 'active'))
  let recentWorkspaceGroups = $derived(groupRecentSessionsByWorkspace(activeWorkspaces, recentSessions))

  $effect(() => {
    if (!renameDialogOpen && renamingSession && renamingSessionId === null) cancelRenameSession()
  })

  function normalizePath(pathname: string) {
    return pathname.replace(/^\/dashboard/, '') || '/'
  }

  function isActive(path: string) {
    if (path === '/chat') return currentPath === '/chat'
    return currentPath === path
  }

  function isSettingsActive() {
    return currentPath === '/settings' || currentPath.startsWith('/settings/')
  }

  function activeSessionIdFromPath(): string | null {
    const match = currentPath.match(/^\/(?:chat|sessions)\/([^/?#]+)/)
    return match ? decodeURIComponent(match[1]) : null
  }

  function isSessionActive(sessionId: string) {
    return activeSessionIdFromPath() === sessionId
  }

  function isSessionVisibleState(state: string) {
    return state !== 'exited'
  }

  function groupRecentSessionsByWorkspace(workspaces: WorkspaceView[], sessions: SessionView[]): RecentWorkspaceGroup[] {
    const sessionsByWorkspaceId = new Map<string, SessionView[]>()
    for (const session of sessions) {
      if (!session.workspace_id) continue
      const workspaceSessions = sessionsByWorkspaceId.get(session.workspace_id) ?? []
      workspaceSessions.push(session)
      sessionsByWorkspaceId.set(session.workspace_id, workspaceSessions)
    }

    return workspaces
      .map((workspace) => ({ workspace, sessions: sessionsByWorkspaceId.get(workspace.workspace_id) ?? [] }))
      .filter((group) => group.sessions.length > 0)
  }

  function isWorkspaceExpanded(workspaceId: string): boolean {
    return expandedWorkspaceIds[workspaceId] ?? false
  }

  function toggleWorkspace(workspaceId: string): void {
    expandedWorkspaceIds = {
      ...expandedWorkspaceIds,
      [workspaceId]: !isWorkspaceExpanded(workspaceId),
    }
  }

  function sessionStateDotClass(state: string) {
    switch (state) {
      case 'busy':
      case 'starting':
        return 'bg-amber-500'
      case 'idle':
      case 'interrupted':
        return 'bg-emerald-500'
      case 'error':
        return 'bg-destructive'
      default:
        return 'bg-muted-foreground'
    }
  }

  function notifyRouteChanged() {
    window.dispatchEvent(new PopStateEvent('popstate'))
  }

  function go(path: string) {
    navigate(path)
    currentPath = normalizePath(path)
    notifyRouteChanged()
  }

  function openSettingsSection(path: string) {
    navigate(path)
    currentPath = path
    notifyRouteChanged()
  }

  function openSession(sessionId: string) {
    navigate(`/chat/${sessionId}`)
    currentPath = `/chat/${sessionId}`
    notifyRouteChanged()
  }

  function startRenamingSession(event: MouseEvent, session: SessionView) {
    event.stopPropagation()
    renamingSession = session
    renameError = null
    renameDialogOpen = true
  }

  async function confirmRenameSession(title: string | null): Promise<void> {
    if (!renamingSession) return
    renamingSessionId = renamingSession.session_id
    renameError = null
    try {
      await updateSessionTitle(renamingSession.session_id, title)
      renameDialogOpen = false
      renamingSession = null
    } catch (error) {
      renameError = error instanceof Error ? error.message : String(error)
    } finally {
      renamingSessionId = null
    }
  }

  function cancelRenameSession(): void {
    renameError = null
    renamingSession = null
  }
</script>

<svelte:window onpopstate={() => (currentPath = normalizePath(window.location.pathname))} />

<Sidebar.Root collapsible="icon">
  <Sidebar.Header>
    <button
      type="button"
      class="flex items-center gap-2 rounded-md px-2 py-2 text-left text-sm font-semibold hover:bg-sidebar-accent group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:p-0"
      onclick={() => go('/chat')}
      aria-label="Open new chat"
    >
      <span class="flex size-8 shrink-0 items-center justify-center rounded-lg">
        <img src="/dashboard/logo.svg" alt="" class="size-6 shrink-0 object-contain" />
      </span>
      <span class="truncate group-data-[collapsible=icon]:hidden">PONTIA</span>
    </button>
  </Sidebar.Header>
  <Sidebar.Content class="overflow-hidden">
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

    <div class="no-scrollbar min-h-0 flex-1 overflow-y-auto group-data-[collapsible=icon]:hidden">
      <Sidebar.Group>
        <Sidebar.GroupLabel>Recent Workspaces</Sidebar.GroupLabel>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#if ($workspacesLoading || $sessionsLoading) && !recentWorkspaceGroups.length}
            <Sidebar.MenuSkeleton />
            <Sidebar.MenuSkeleton />
          {:else if recentWorkspaceGroups.length}
            {#each recentWorkspaceGroups as group (group.workspace.workspace_id)}
              {@const workspace = group.workspace}
              {@const workspaceExpanded = isWorkspaceExpanded(workspace.workspace_id)}
              <Sidebar.MenuItem>
                <button
                  type="button"
                  class="flex h-8 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-sm text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
                  title={workspace.canonical_path}
                  aria-expanded={workspaceExpanded}
                  onclick={() => toggleWorkspace(workspace.workspace_id)}
                >
                  <Folder class="size-4 shrink-0" />
                  <span class="truncate">{workspaceTitle(workspace)}</span>
                  <ChevronDown class={cn('ml-auto size-4 shrink-0 transition-transform', workspaceExpanded ? 'rotate-0' : '-rotate-90')} />
                </button>
                {#if workspaceExpanded}
                  <Sidebar.Menu class="mt-1 pl-2">
                    {#each group.sessions as session (session.session_id)}
                      <Sidebar.MenuItem>
                        <Sidebar.MenuButton isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session)} · ${session.state}`} onclick={() => openSession(session.session_id)}>
                          {#if isSessionVisibleState(session.state)}
                            <span class={`size-2 shrink-0 rounded-full ${sessionStateDotClass(session.state)} group-data-[collapsible=icon]:hidden`} aria-label={`${session.state} session`}></span>
                          {/if}
                          <span class="line-clamp-1">{sessionChatTitle(session)}</span>
                        </Sidebar.MenuButton>
                      </Sidebar.MenuItem>
                    {/each}
                  </Sidebar.Menu>
                {/if}
              </Sidebar.MenuItem>
            {/each}
          {:else}
            <Sidebar.MenuItem>
              <div class="px-2 py-1 text-xs text-sidebar-foreground/60">No recent workspaces</div>
            </Sidebar.MenuItem>
          {/if}
        </Sidebar.Menu>
        </Sidebar.GroupContent>
      </Sidebar.Group>

      <Sidebar.Group>
        <Sidebar.GroupLabel class="p-0">
        <button
          type="button"
          class="flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-xs font-medium hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
          aria-expanded={recentSessionsOpen}
          onclick={() => (recentSessionsOpen = !recentSessionsOpen)}
        >
          <span>Recent Sessions</span>
          <ChevronDown class={cn('size-4 transition-transform', recentSessionsOpen ? 'rotate-0' : '-rotate-90')} />
        </button>
      </Sidebar.GroupLabel>
      {#if recentSessionsOpen}
        <Sidebar.GroupContent class="pr-1">
          <Sidebar.Menu>
            {#if $sessionsLoading && !recentSessions.length}
              <Sidebar.MenuSkeleton />
              <Sidebar.MenuSkeleton />
            {:else if recentSessions.length}
              {#each recentSessions as session}
                <Sidebar.MenuItem>
                  <Sidebar.MenuButton class="group-has-data-[sidebar=menu-action]/menu-item:pr-8" isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session)} · ${session.state}`} onclick={() => openSession(session.session_id)}>
                    {#if isSessionVisibleState(session.state)}
                      <span class={`size-2 shrink-0 rounded-full ${sessionStateDotClass(session.state)} group-data-[collapsible=icon]:hidden`} aria-label={`${session.state} session`}></span>
                    {/if}
                    <span class="line-clamp-1">{sessionChatTitle(session)}</span>
                  </Sidebar.MenuButton>
                  <Sidebar.MenuAction
                    showOnHover
                    aria-label={`Rename session ${sessionChatTitle(session)}`}
                    title="Rename session"
                    disabled={renamingSessionId === session.session_id}
                    onclick={(event) => startRenamingSession(event, session)}
                  >
                    <Pencil />
                  </Sidebar.MenuAction>
                </Sidebar.MenuItem>
              {/each}
            {:else}
              <Sidebar.MenuItem>
                <div class="px-2 py-1 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">No active sessions</div>
              </Sidebar.MenuItem>
            {/if}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      {/if}
      </Sidebar.Group>
    </div>
  </Sidebar.Content>
  <Sidebar.Footer>
    <Sidebar.Menu>
      <Sidebar.MenuItem>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            data-active={isSettingsActive() ? true : undefined}
            class={cn(sidebarMenuButtonVariants(), 'w-full')}
            aria-label="Settings"
          >
            <Settings />
            <span>Settings</span>
          </DropdownMenu.Trigger>
          <DropdownMenu.Content side="top" align="end" class="w-48">
            <DropdownMenu.Label>Settings</DropdownMenu.Label>
            <DropdownMenu.Separator />
            {#each settingsSections as section}
              <DropdownMenu.Item onclick={() => openSettingsSection(section.path)}>
                {section.label}
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </Sidebar.MenuItem>
    </Sidebar.Menu>
  </Sidebar.Footer>
  <Sidebar.Rail />
</Sidebar.Root>

<RenameSessionDialog
  bind:open={renameDialogOpen}
  session={renamingSession}
  busy={renamingSessionId !== null}
  error={renameError}
  onConfirm={(title) => void confirmRenameSession(title)}
  onCancel={cancelRenameSession}
/>
