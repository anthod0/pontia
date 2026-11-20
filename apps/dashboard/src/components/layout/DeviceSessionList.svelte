<script lang="ts">
  import { statusLabel } from '$lib/i18n/status';
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import ArchiveIcon from 'phosphor-svelte/lib/ArchiveIcon'
  import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import FunnelIcon from 'phosphor-svelte/lib/FunnelIcon'
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon'
  import PushPinSlashIcon from 'phosphor-svelte/lib/PushPinSlashIcon'
  import { dashboardRelativePath, dashboardScope } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { cn } from '$lib/utils.js'
  import { recentSessions, snapshotSessionOverview } from "../../queries/sessionOverview";
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionStateDotClass } from '$lib/sessionState'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import type { SessionView } from '../../api/types'
  const device = useDeviceRuntime();
  const { archiveSession, pinSession, terminateSession, unpinSession, updateSessionTitle } = device.sessions;
  const { createSessionOverviewQuery } = device.sessionOverview;
  const { createWorkspacesQuery } = device.workspaceQueries;

  let { handle }: { handle?: string } = $props()

  let currentPath = $state(window.location.pathname)
  let renamingSessionId = $state<string | null>(null)
  let renamingSession = $state<SessionView | null>(null)
  let renameDialogOpen = $state(false)
  let renameError = $state<string | null>(null)
  let sessionManagementBusyId = $state<string | null>(null)
  let sessionActionMenuOpenKey = $state<string | null>(null)
  let pinnedSessionsOpen = $state(true)
  let recentSessionsOpen = $state(true)
  const { selectedWorkspaceId } = device.sidebarWorkspaceFilter
  const workspacesQuery = createWorkspacesQuery()
  const selectedWorkspace = $derived(workspacesQuery.data?.find((workspace) => workspace.workspace_id === $selectedWorkspaceId))
  const overviewQuery = createSessionOverviewQuery(false, () => $selectedWorkspaceId || undefined)
  const overview = $derived(snapshotSessionOverview(overviewQuery.data))
  const sidebarRecentSessions = $derived(recentSessions(overview, $selectedWorkspaceId))

  $effect(() => {
    if (!renameDialogOpen && renamingSession && renamingSessionId === null) cancelRenameSession()
  })

  function activeSessionIdFromPath(): string | null {
    const match = dashboardRelativePath(currentPath).match(/^\/chat\/([^/?#]+)/)
    return match ? decodeURIComponent(match[1]) : null
  }

  function isSessionActive(sessionId: string) {
    return (handle === undefined || dashboardScope(currentPath) === `/${handle}`) && activeSessionIdFromPath() === sessionId
  }

  function isSessionVisibleState(state: string) {
    return state !== 'exited' && state !== 'error'
  }

  function isTerminalSessionState(state: string) {
    return state === 'exited' || state === 'error'
  }

  function go(path: string) {
    if (handle === undefined) void navigate(path)
    else void navigate(path, undefined, { scope: `/${handle}` })
  }

  function openSession(sessionId: string) {
    go(`/chat/${sessionId}`)
  }

  function setSessionActionMenuOpen(actionKey: string, open: boolean): void {
    sessionActionMenuOpenKey = open ? actionKey : null
  }

  function startRenamingSession(session: SessionView) {
    renamingSession = session
    renameError = null
    renameDialogOpen = true
  }

  async function togglePinSession(session: SessionView): Promise<void> {
    sessionManagementBusyId = session.session_id
    try {
      if (session.pinned_at) await unpinSession(session.session_id)
      else await pinSession(session.session_id)
    } finally {
      sessionManagementBusyId = null
    }
  }

  async function archiveSessionFromSidebar(session: SessionView): Promise<void> {
    sessionManagementBusyId = session.session_id
    try {
      await archiveSession(session.session_id)
    } finally {
      sessionManagementBusyId = null
    }
  }

  async function exitSessionFromSidebar(session: SessionView): Promise<void> {
    sessionManagementBusyId = session.session_id
    try {
      await terminateSession(session.session_id)
    } finally {
      sessionManagementBusyId = null
    }
  }

  async function togglePinSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await togglePinSession(session)
  }

  async function archiveSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await archiveSessionFromSidebar(session)
  }

  async function exitSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await exitSessionFromSidebar(session)
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

  function loadNextPage(): void {
    if (overview.nextCursor && !overviewQuery.isFetchingNextPage) void overviewQuery.fetchNextPage()
  }

  // Re-observing after each page reports whether the end is still in view, so loading continues until the list
  // fills the sidebar. A failed page is retried only when the end scrolls back into view.
  function observeNextPageSentinel(node: HTMLElement): { destroy: () => void } {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) loadNextPage()
    }, { root: node.closest('[data-sidebar="content"]'), rootMargin: '0px 0px 80px 0px' })
    $effect(() => {
      if (overview.nextCursor && !overviewQuery.isFetchingNextPage && !overviewQuery.isFetchNextPageError) {
        observer.unobserve(node)
        observer.observe(node)
      }
    })
    observer.observe(node)
    return { destroy: () => observer.disconnect() }
  }
</script>

<svelte:window onpopstate={() => (currentPath = window.location.pathname)} />

{#snippet sessionMenuItem(session: SessionView, actionKey: string)}
  <Sidebar.MenuItem>
    <Sidebar.MenuButton class="group-has-data-[sidebar=menu-action]/menu-item:pr-8" isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session, $t)} · ${statusLabel(session.state, $t)}`} onclick={() => openSession(session.session_id)}>
      {#if session.pinned_at}
        <PushPinIcon class="size-3.5 shrink-0" aria-label={$t('sessions.pinnedSession')} />
      {/if}
      <span class="line-clamp-1">{sessionChatTitle(session, $t)}</span>
    </Sidebar.MenuButton>
    {#if isSessionVisibleState(session.state)}
      <span
        class={cn('pointer-events-none absolute top-1.5 right-1 flex aspect-square w-5 items-center justify-center transition-opacity group-has-[:focus-visible]/menu-item:opacity-0 group-hover/menu-item:opacity-0 group-data-[collapsible=icon]:hidden', sessionActionMenuOpenKey === actionKey ? 'opacity-0' : 'opacity-100')}
        aria-label={$t('sessions.state', { state: statusLabel(session.state, $t) })}
      >
        <span class={`size-2 rounded-full ${sessionStateDotClass(session.state)}`}></span>
      </span>
    {/if}
    <DropdownMenu.Root bind:open={() => sessionActionMenuOpenKey === actionKey, (open) => setSessionActionMenuOpen(actionKey, open)}>
      <Sidebar.MenuAction
        showOnHover
        aria-label={$t('sessions.actionsFor', { name: sessionChatTitle(session, $t) })}
        title={$t('sessions.actions')}
        disabled={renamingSessionId === session.session_id || sessionManagementBusyId === session.session_id}
        onclick={(event) => event.stopPropagation()}
      >
        {#snippet child({ props })}
          <DropdownMenu.Trigger {...props}>
            <DotsThreeVerticalIcon />
          </DropdownMenu.Trigger>
        {/snippet}
      </Sidebar.MenuAction>
      <DropdownMenu.Content side="right" align="start" class="w-44">
        <DropdownMenu.Item onclick={() => startRenamingSession(session)}>
          <PencilSimpleIcon class="size-4" /> {$t('common.rename')}
        </DropdownMenu.Item>
        <DropdownMenu.Item disabled={sessionManagementBusyId === session.session_id} onclick={() => void togglePinSessionFromMenu(session)}>
          {#if session.pinned_at}
            <PushPinSlashIcon class="size-4" /> {$t('sessions.unpin')}
          {:else}
            <PushPinIcon class="size-4" /> {$t('sessions.pin')}
          {/if}
        </DropdownMenu.Item>
        <DropdownMenu.Item disabled={sessionManagementBusyId === session.session_id} onclick={() => void archiveSessionFromMenu(session)}>
          <ArchiveIcon class="size-4" /> {$t('common.archive')}
        </DropdownMenu.Item>
        {#if !isTerminalSessionState(session.state)}
          <DropdownMenu.Separator />
          <DropdownMenu.Item variant="destructive" disabled={sessionManagementBusyId === session.session_id} onclick={() => void exitSessionFromMenu(session)}>
            <SignOutIcon class="size-4" /> {$t('common.exit')}
          </DropdownMenu.Item>
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </Sidebar.MenuItem>
{/snippet}

<div class="group-data-[collapsible=icon]:hidden">
  {#if overviewQuery.error}
    <p role="alert" class="px-4 py-2 text-xs text-destructive">{$t('sessions.sidebarFailed')} {overviewQuery.error.message}</p>
  {/if}
  {#if overview.pinned.length}
    <Sidebar.Group>
      <Sidebar.GroupLabel class="sticky top-0 z-10 flex h-8 items-center gap-1 bg-sidebar p-0 px-2">
        <button
          type="button"
          class="flex min-w-0 items-center gap-1 rounded-sm text-left text-xs font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
          aria-expanded={pinnedSessionsOpen}
          onclick={() => (pinnedSessionsOpen = !pinnedSessionsOpen)}
        >
          <span>{$t('sessions.pinnedTitle')}</span>
          <CaretRightIcon class={cn('size-3 transition-transform', pinnedSessionsOpen && 'rotate-90')} />
        </button>
      </Sidebar.GroupLabel>
      {#if pinnedSessionsOpen}
        <Sidebar.GroupContent class="pr-1">
          <Sidebar.Menu>
            {#each overview.pinned as session}
              {@render sessionMenuItem(session, `pinned:${session.session_id}`)}
            {/each}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      {/if}
    </Sidebar.Group>
  {/if}
  <Sidebar.Group>
    <Sidebar.GroupLabel class="sticky top-0 z-10 flex h-8 items-center gap-1 bg-sidebar p-0 px-2">
      <button
        type="button"
        class="flex min-w-0 items-center gap-1 rounded-sm text-left text-xs font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
        aria-expanded={recentSessionsOpen}
        onclick={() => (recentSessionsOpen = !recentSessionsOpen)}
      >
        <span>{$t('sessions.recentTitle')}</span>
        <CaretRightIcon class={cn('size-3 transition-transform', recentSessionsOpen && 'rotate-90')} />
      </button>
      <span class="flex-1"></span>
      <span class="flex items-center gap-2">
        <button
          type="button"
          class="rounded-sm hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
          aria-label={$t('sessions.openAll')}
          title={$t('sessions.all')}
          onclick={() => go('/sessions')}
        >
          <ArrowUpRightIcon class="size-4" />
        </button>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class={cn('rounded-sm hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden', $selectedWorkspaceId && 'text-primary')}
            aria-label={$t('sessions.filterWorkspace')}
            title={$selectedWorkspaceId ? `Workspace: ${selectedWorkspace?.name || selectedWorkspace?.display_path || $selectedWorkspaceId}` : $t('workspaces.all')}
          >
            <FunnelIcon class="size-4" weight={$selectedWorkspaceId ? 'fill' : 'regular'} />
          </DropdownMenu.Trigger>
          <DropdownMenu.Content side="right" align="start" class="max-h-80 w-72 overflow-y-auto">
            <DropdownMenu.Label>{$t('workspace.title')}</DropdownMenu.Label>
            <DropdownMenu.RadioGroup bind:value={$selectedWorkspaceId}>
              <DropdownMenu.RadioItem value="">{$t('workspaces.all')}</DropdownMenu.RadioItem>
              <DropdownMenu.Separator />
              {#each workspacesQuery.data ?? [] as workspace (workspace.workspace_id)}
                <DropdownMenu.RadioItem value={workspace.workspace_id}>
                  <span class="min-w-0" title={workspace.canonical_path}>
                    <span class="block truncate">{workspace.name || workspace.display_path}</span>
                    {#if workspace.name}
                      <span class="block truncate text-xs text-muted-foreground">{workspace.display_path}</span>
                    {/if}
                  </span>
                </DropdownMenu.RadioItem>
              {/each}
            </DropdownMenu.RadioGroup>
            {#if workspacesQuery.error}
              <p role="alert" class="px-2 py-1 text-xs text-destructive">{$t('workspaces.loadFailedMessage')}</p>
              <DropdownMenu.Item onclick={() => void workspacesQuery.refetch()}>{$t('common.retry')}</DropdownMenu.Item>
            {:else if workspacesQuery.isPending}
              <p role="status" class="px-2 py-1 text-xs text-muted-foreground">{$t('workspaces.loading')}</p>
            {:else if !workspacesQuery.data?.length}
              <p class="px-2 py-1 text-xs text-muted-foreground">{$t('workspaces.empty')}</p>
            {/if}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </span>
    </Sidebar.GroupLabel>
  {#if recentSessionsOpen}
    <Sidebar.GroupContent class="pr-1">
      <Sidebar.Menu>
        {#if overviewQuery.isPending && !sidebarRecentSessions.length}
          <Sidebar.MenuSkeleton />
          <Sidebar.MenuSkeleton />
        {:else if sidebarRecentSessions.length}
          {#each sidebarRecentSessions as session}
            {@render sessionMenuItem(session, `recent:${session.session_id}`)}
          {/each}
        {:else if !overviewQuery.error}
          <Sidebar.MenuItem>
            <div class="px-2 py-1 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">{$t('sessions.noRecent')}</div>
          </Sidebar.MenuItem>
        {/if}
        {#if overviewQuery.isFetchingNextPage}
          <Sidebar.MenuSkeleton />
        {/if}
      </Sidebar.Menu>
      <div aria-hidden="true" class="h-px" use:observeNextPageSentinel></div>
    </Sidebar.GroupContent>
  {/if}
  </Sidebar.Group>
</div>

<RenameSessionDialog
  bind:open={renameDialogOpen}
  session={renamingSession}
  busy={renamingSessionId !== null}
  error={renameError}
  onConfirm={(title) => void confirmRenameSession(title)}
  onCancel={cancelRenameSession}
/>
