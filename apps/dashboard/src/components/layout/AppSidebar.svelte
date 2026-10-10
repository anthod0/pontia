<script lang="ts">
  import ArchiveIcon from 'phosphor-svelte/lib/ArchiveIcon'
  import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import FunnelIcon from 'phosphor-svelte/lib/FunnelIcon'
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon'
  import PushPinSlashIcon from 'phosphor-svelte/lib/PushPinSlashIcon'
  import GearIcon from 'phosphor-svelte/lib/GearIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import { base } from '$app/paths'
  import { dashboardRelativePath } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as Kbd from '$lib/components/ui/kbd/index.js'
  import { cn } from '$lib/utils.js'
  import { archiveSession, loadMoreSidebarSessions, pinSession, sidebarActiveSessions, sidebarPinnedSessions, sidebarRecentSessions, sidebarSessionsError, sidebarSessionsLoading, sidebarSessionsLoadingMore, sidebarSessionsNextCursor, terminateSession, unpinSession, updateSessionTitle } from '../../stores/sessions'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionStateDotClass } from '$lib/sessionState'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import type { SessionView } from '../../api/types'

  type Item = {
    label: string
    path: string
    icon: typeof NotePencilIcon
  }

  const primaryItems: Item[] = [
    { label: 'New Chat', path: '/', icon: NotePencilIcon },
    { label: 'Workflows', path: '/workflows', icon: TreeStructureIcon },
  ]

  let currentPath = $state(dashboardRelativePath())
  let renamingSessionId = $state<string | null>(null)
  let renamingSession = $state<SessionView | null>(null)
  let renameDialogOpen = $state(false)
  let renameError = $state<string | null>(null)
  let sessionManagementBusyId = $state<string | null>(null)
  let sessionActionMenuOpenKey = $state<string | null>(null)
  let pinnedSessionsOpen = $state(true)
  let recentSessionsOpen = $state(true)
  let recentSessions = $derived.by(() => {
    const activeIds = new Set($sidebarActiveSessions.map((session) => session.session_id))
    return [
      ...$sidebarActiveSessions,
      ...$sidebarRecentSessions.filter((session) => !activeIds.has(session.session_id)),
    ]
  })

  $effect(() => {
    if (!renameDialogOpen && renamingSession && renamingSessionId === null) cancelRenameSession()
  })

  function isActive(path: string) {
    return currentPath === path || (path !== '/' && currentPath.startsWith(`${path}/`))
  }

  function isSettingsActive() {
    return currentPath === '/settings' || currentPath.startsWith('/settings/')
  }

  function activeSessionIdFromPath(): string | null {
    const match = currentPath.match(/^\/chat\/([^/?#]+)/)
    return match ? decodeURIComponent(match[1]) : null
  }

  function isSessionActive(sessionId: string) {
    return activeSessionIdFromPath() === sessionId
  }

  function isSessionVisibleState(state: string) {
    return state !== 'exited' && state !== 'error'
  }

  function isTerminalSessionState(state: string) {
    return state === 'exited' || state === 'error'
  }

  function notifyRouteChanged() {
    window.dispatchEvent(new PopStateEvent('popstate'))
  }

  function go(path: string) {
    navigate(path)
    currentPath = path
    notifyRouteChanged()
  }

  function openSession(sessionId: string) {
    navigate(`/chat/${sessionId}`)
    currentPath = `/chat/${sessionId}`
    notifyRouteChanged()
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

  function handleSessionListScroll(event: Event): void {
    const target = event.currentTarget as HTMLElement
    if (
      target.scrollHeight - target.scrollTop - target.clientHeight <= 80 &&
      $sidebarSessionsNextCursor &&
      !$sidebarSessionsLoadingMore
    ) {
      void loadMoreSidebarSessions()
    }
  }
</script>

<svelte:window onpopstate={() => (currentPath = dashboardRelativePath())} />

{#snippet shortcutHint(key: string)}
  <Kbd.Group class="ml-auto shrink-0 text-[10px] text-muted-foreground group-data-[collapsible=icon]:hidden" aria-hidden="true">
    <Kbd.Root class="px-1 py-0 text-[10px]">Alt</Kbd.Root>
    <Kbd.Root class="px-1 py-0 text-[10px]">{key}</Kbd.Root>
  </Kbd.Group>
{/snippet}

{#snippet sessionMenuItem(session: SessionView, actionKey: string)}
  <Sidebar.MenuItem>
    <Sidebar.MenuButton class="group-has-data-[sidebar=menu-action]/menu-item:pr-8" isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session)} · ${session.state}`} onclick={() => openSession(session.session_id)}>
      {#if session.pinned_at}
        <PushPinIcon class="size-3.5 shrink-0" aria-label="Pinned session" />
      {/if}
      <span class="line-clamp-1">{sessionChatTitle(session)}</span>
    </Sidebar.MenuButton>
    {#if isSessionVisibleState(session.state)}
      <span
        class={cn('pointer-events-none absolute top-1.5 right-1 flex aspect-square w-5 items-center justify-center transition-opacity group-has-[:focus-visible]/menu-item:opacity-0 group-hover/menu-item:opacity-0 group-data-[collapsible=icon]:hidden', sessionActionMenuOpenKey === actionKey ? 'opacity-0' : 'opacity-100')}
        aria-label={`${session.state} session`}
      >
        <span class={`size-2 rounded-none ${sessionStateDotClass(session.state)}`}></span>
      </span>
    {/if}
    <DropdownMenu.Root bind:open={() => sessionActionMenuOpenKey === actionKey, (open) => setSessionActionMenuOpen(actionKey, open)}>
      <Sidebar.MenuAction
        showOnHover
        aria-label={`Open session actions for ${sessionChatTitle(session)}`}
        title="Session actions"
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
          <PencilSimpleIcon class="size-4" /> Rename
        </DropdownMenu.Item>
        <DropdownMenu.Item disabled={sessionManagementBusyId === session.session_id} onclick={() => void togglePinSessionFromMenu(session)}>
          {#if session.pinned_at}
            <PushPinSlashIcon class="size-4" /> Unpin
          {:else}
            <PushPinIcon class="size-4" /> Pin
          {/if}
        </DropdownMenu.Item>
        <DropdownMenu.Item disabled={sessionManagementBusyId === session.session_id} onclick={() => void archiveSessionFromMenu(session)}>
          <ArchiveIcon class="size-4" /> Archive
        </DropdownMenu.Item>
        {#if !isTerminalSessionState(session.state)}
          <DropdownMenu.Separator />
          <DropdownMenu.Item variant="destructive" disabled={sessionManagementBusyId === session.session_id} onclick={() => void exitSessionFromMenu(session)}>
            <SignOutIcon class="size-4" /> Exit
          </DropdownMenu.Item>
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </Sidebar.MenuItem>
{/snippet}

<Sidebar.Root collapsible="icon">
  <Sidebar.Header class="px-2 py-0">
    <button
      type="button"
      class="flex h-14 items-center gap-3 px-2 text-left text-sm font-semibold text-heading hover:bg-sidebar-accent group-data-[collapsible=icon]:px-0"
      onclick={() => go('/')}
      aria-label="Open new chat"
    >
      <span class="flex size-8 shrink-0 items-center justify-center rounded-none">
        <img src={`${base}/logo.svg`} alt="" class="size-8 shrink-0 object-contain" />
      </span>
      <span class="truncate group-data-[collapsible=icon]:hidden">Pontia</span>
    </button>
  </Sidebar.Header>
  <Sidebar.Separator class="mx-4 data-[orientation=horizontal]:w-auto group-data-[collapsible=icon]:mx-2" />
  <Sidebar.Content class="overflow-hidden">
    <Sidebar.Group>
      <Sidebar.GroupContent>
        <Sidebar.Menu>
          {#each primaryItems as item}
            <Sidebar.MenuItem>
              <Sidebar.MenuButton
                isActive={isActive(item.path)}
                class={item.path === '/' ? 'mb-2 h-10 justify-center bg-primary text-[13px] font-semibold text-primary-foreground hover:bg-primary-hover hover:text-primary-foreground active:bg-primary-active data-[active=true]:bg-primary data-[active=true]:text-primary-foreground data-[active=true]:hover:bg-primary-hover' : 'h-9 px-3 text-[13px]'}
                tooltipContent={item.label}
                onclick={() => go(item.path)}
              >
                <item.icon class={item.path === '/workflows' ? 'text-aqua' : ''} />
                <span>{item.label}</span>
                {#if item.path === '/'}
                  {@render shortcutHint('N')}
                {/if}
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
          {/each}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton
              isActive={isSettingsActive()}
              class="h-9 px-3 text-[13px]"
              tooltipContent="Settings"
              onclick={() => go('/settings/common')}
            >
              <GearIcon />
              <span>Settings</span>
            </Sidebar.MenuButton>
          </Sidebar.MenuItem>
        </Sidebar.Menu>
      </Sidebar.GroupContent>
    </Sidebar.Group>

    <div class="no-scrollbar min-h-0 flex-1 overflow-y-auto group-data-[collapsible=icon]:hidden" onscroll={handleSessionListScroll}>
      {#if $sidebarSessionsError}
        <p role="alert" class="px-4 py-2 text-xs text-destructive">Sidebar refresh failed. {$sidebarSessionsError}</p>
      {/if}
      <Sidebar.Group>
        <Sidebar.GroupLabel class="flex h-8 items-center gap-1 p-0 px-2">
          <button
            type="button"
            class="flex min-w-0 items-center gap-1 rounded-none text-left text-xs font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
            aria-expanded={pinnedSessionsOpen}
            onclick={() => (pinnedSessionsOpen = !pinnedSessionsOpen)}
          >
            <span>Pinned Sessions</span>
            <CaretRightIcon class={cn('size-3 transition-transform', pinnedSessionsOpen && 'rotate-90')} />
          </button>
        </Sidebar.GroupLabel>
        {#if pinnedSessionsOpen}
          <Sidebar.GroupContent class="pr-1">
            <Sidebar.Menu>
              {#if $sidebarSessionsLoading && !$sidebarPinnedSessions.length}
                <Sidebar.MenuSkeleton />
              {:else if $sidebarPinnedSessions.length}
                {#each $sidebarPinnedSessions as session}
                  {@render sessionMenuItem(session, `pinned:${session.session_id}`)}
                {/each}
              {:else if !$sidebarSessionsError}
                <Sidebar.MenuItem>
                  <div class="px-2 py-1 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">No pinned sessions</div>
                </Sidebar.MenuItem>
              {/if}
            </Sidebar.Menu>
          </Sidebar.GroupContent>
        {/if}
      </Sidebar.Group>
      <Sidebar.Group>
        <Sidebar.GroupLabel class="flex h-8 items-center gap-1 p-0 px-2">
          <button
            type="button"
            class="flex min-w-0 items-center gap-1 rounded-none text-left text-xs font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
            aria-expanded={recentSessionsOpen}
            onclick={() => (recentSessionsOpen = !recentSessionsOpen)}
          >
            <span>Recent Sessions</span>
            <CaretRightIcon class={cn('size-3 transition-transform', recentSessionsOpen && 'rotate-90')} />
          </button>
          <span class="flex-1"></span>
          <span class="flex items-center gap-2">
            <button
              type="button"
              class="rounded-none hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
              aria-label="Open all sessions"
              title="All sessions"
              onclick={() => go('/sessions')}
            >
              <ArrowUpRightIcon class="size-4" />
            </button>
            <FunnelIcon class="size-4" aria-hidden="true" />
          </span>
        </Sidebar.GroupLabel>
      {#if recentSessionsOpen}
        <Sidebar.GroupContent class="pr-1">
          <Sidebar.Menu>
            {#if $sidebarSessionsLoading && !recentSessions.length}
              <Sidebar.MenuSkeleton />
              <Sidebar.MenuSkeleton />
            {:else if recentSessions.length}
              {#each recentSessions as session}
                {@render sessionMenuItem(session, `recent:${session.session_id}`)}
              {/each}
            {:else if !$sidebarSessionsError}
              <Sidebar.MenuItem>
                <div class="px-2 py-1 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">No recent sessions</div>
              </Sidebar.MenuItem>
            {/if}
            {#if $sidebarSessionsLoadingMore}
              <Sidebar.MenuSkeleton />
            {/if}
          </Sidebar.Menu>
        </Sidebar.GroupContent>
      {/if}
      </Sidebar.Group>
    </div>
  </Sidebar.Content>
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
