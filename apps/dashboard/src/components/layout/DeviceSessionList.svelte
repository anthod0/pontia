<script lang="ts">
  import ResponsiveMenu from '$lib/components/ui/overlay/ResponsiveMenu.svelte'
  import { statusLabel } from '$lib/i18n/status';
  import { t } from '$lib/i18n';
  import { useDeviceRuntime } from '../../device/context';
  import ArchiveIcon from 'phosphor-svelte/lib/ArchiveIcon'
  import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import ClockCounterClockwiseIcon from 'phosphor-svelte/lib/ClockCounterClockwiseIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import FunnelIcon from 'phosphor-svelte/lib/FunnelIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon'
  import PushPinSlashIcon from 'phosphor-svelte/lib/PushPinSlashIcon'
  import { dashboardRelativePath, dashboardScope } from '$lib/dashboardRoutes'
  import { navigate } from '$lib/navigation'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import ShortcutKeys from '$lib/components/shortcuts/ShortcutKeys.svelte'
  import { cn } from '$lib/utils.js'
  import { createSessionList } from '../../features/sessions/sessionList.svelte'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { sessionStateDotClass } from '$lib/sessionState'
  import ClientIcon from '../chat/ClientIcon.svelte'
  import RenameSessionDialog from '../chat/RenameSessionDialog.svelte'
  import type { SessionView } from '../../api/types'
  const device = useDeviceRuntime();
  const sidebar = Sidebar.useSidebar();
  const { createWorkspacesQuery } = device.workspaceQueries;

  let { handle, onNewChat }: { handle?: string; onNewChat: () => void } = $props()

  let currentPath = $state(window.location.pathname)
  let renamingSessionId = $state<string | null>(null)
  let renamingSession = $state<SessionView | null>(null)
  let renameDialogOpen = $state(false)
  let renameError = $state<string | null>(null)
  let filterTrigger = $state<HTMLButtonElement | null>(null)
  let sessionActionMenuOpenKey = $state<string | null>(null)
  let sessionsOpen = $state(true)
  const { selectedWorkspaceId, sessionScope } = device.sidebarSessionFilter
  const workspacesQuery = createWorkspacesQuery()
  const selectedWorkspace = $derived(workspacesQuery.data?.find((workspace) => workspace.workspace_id === $selectedWorkspaceId))
  const sessions = createSessionList(device, false, () => $selectedWorkspaceId, () => $sessionScope)
  const overviewQuery = sessions.query
  const overview = $derived(sessions.overview)
  const sidebarRecentSessions = $derived(sessions.recent)
  const hasClosedSessions = $derived(sessions.hasClosed)
  const headerButtonClass = 'inline-flex size-6 shrink-0 items-center justify-center rounded-full p-0 text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden [&>svg]:size-[15px]'
  const filterApplied = $derived(Boolean($selectedWorkspaceId) || $sessionScope === 'all')
  const filterTitle = $derived([
    $sessionScope === 'active' ? $t('sessions.activeOnly') : $t('sessions.all'),
    $selectedWorkspaceId ? selectedWorkspace?.name || selectedWorkspace?.display_path || $selectedWorkspaceId : $t('workspaces.all'),
  ].join(' · '))

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

  async function togglePinSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await sessions.pin(session)
  }

  async function archiveSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await sessions.archive(session)
  }

  async function exitSessionFromMenu(session: SessionView): Promise<void> {
    sessionActionMenuOpenKey = null
    await sessions.exit(session)
  }

  async function confirmRenameSession(title: string | null): Promise<void> {
    if (!renamingSession) return
    renamingSessionId = renamingSession.session_id
    renameError = null
    try {
      await sessions.rename(renamingSession, title)
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

  // Re-observing after each page reports whether the end is still in view, so loading continues until the list
  // fills the sidebar. A failed page is retried only when the end scrolls back into view.
  function observeNextPageSentinel(node: HTMLElement): { destroy: () => void } {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) sessions.loadNextPage()
    }, { root: node.closest('[data-sidebar="content"]'), rootMargin: '0px 0px 80px 0px' })
    $effect(() => {
      if ($sessionScope === 'all' && overview.nextCursor && !overviewQuery.isFetchingNextPage && !overviewQuery.isFetchNextPageError) {
        observer.unobserve(node)
        observer.observe(node)
      }
    })
    observer.observe(node)
    return { destroy: () => observer.disconnect() }
  }
</script>

<svelte:window onpopstate={() => (currentPath = window.location.pathname)} />

{#snippet sessionMenuItems(Item: typeof DropdownMenu.Item, Separator: typeof DropdownMenu.Separator, session: SessionView)}
  <Item onclick={() => startRenamingSession(session)}>
    <PencilSimpleIcon class="size-4" /> {$t('common.rename')}
  </Item>
  <Item disabled={sessions.busyId === session.session_id} onclick={() => void togglePinSessionFromMenu(session)}>
    {#if session.pinned_at}
      <PushPinSlashIcon class="size-4" /> {$t('sessions.unpin')}
    {:else}
      <PushPinIcon class="size-4" /> {$t('sessions.pin')}
    {/if}
  </Item>
  <Item disabled={sessions.busyId === session.session_id} onclick={() => void archiveSessionFromMenu(session)}>
    <ArchiveIcon class="size-4" /> {$t('common.archive')}
  </Item>
  {#if !isTerminalSessionState(session.state)}
    <Separator />
    <Item variant="destructive" disabled={sessions.busyId === session.session_id} onclick={() => void exitSessionFromMenu(session)}>
      <SignOutIcon class="size-4" /> {$t('common.exit')}
    </Item>
  {/if}
{/snippet}

{#snippet sessionMenuItem(session: SessionView, actionKey: string)}
  {@const acpAgentId = (session.acp as { agent_id?: string } | undefined)?.agent_id}
  <Sidebar.MenuItem>
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        {#snippet child({ props })}
          <Sidebar.MenuButton {...props} class="touch-target:group-has-data-[sidebar=menu-action]/menu-item:pr-16 touch-target:h-11" isActive={isSessionActive(session.session_id)} tooltipContent={`${sessionChatTitle(session, $t)} · ${statusLabel(session.state, $t)}`} onclick={() => openSession(session.session_id)}>
            <ClientIcon clientType={session.client_type} {acpAgentId} />
            {#if session.pinned_at}
              <PushPinIcon class="size-3.5 shrink-0" aria-label={$t('sessions.pinnedSession')} />
            {/if}
            <span class="line-clamp-1">{sessionChatTitle(session, $t)}</span>
          </Sidebar.MenuButton>
        {/snippet}
      </ContextMenu.Trigger>
      <ContextMenu.Content aria-label={sessionChatTitle(session, $t)} class="w-44">
        {@render sessionMenuItems(ContextMenu.Item, ContextMenu.Separator, session)}
      </ContextMenu.Content>
    </ContextMenu.Root>
    {#if isSessionVisibleState(session.state)}
      <span
        class={cn('pointer-events-none absolute top-1.5 right-1 touch-target:top-3 touch-target:right-11 flex aspect-square w-5 items-center justify-center transition-opacity group-has-[:focus-visible]/menu-item:opacity-0 group-hover/menu-item:opacity-0 group-data-[collapsible=icon]:hidden', sessionActionMenuOpenKey === actionKey ? 'opacity-0' : 'opacity-100')}
        aria-label={$t('sessions.state', { state: statusLabel(session.state, $t) })}
      >
        <span class={`size-2 rounded-full ${sessionStateDotClass(session.state)}`}></span>
      </span>
    {/if}
    <ResponsiveMenu bind:open={() => sessionActionMenuOpenKey === actionKey, (open) => setSessionActionMenuOpen(actionKey, open)} label={sessionChatTitle(session, $t)} side="right" align="start" sideOffset={4} class="w-44">
      {#snippet trigger()}
        <Sidebar.MenuAction
          showOnHover
          class="touch-target:peer-data-[size=default]/menu-button:top-0 touch-target:right-0 touch-target:size-11 touch-target:after:hidden"
          aria-label={$t('sessions.actionsFor', { name: sessionChatTitle(session, $t) })}
          title={$t('sessions.actions')}
          disabled={renamingSessionId === session.session_id || sessions.busyId === session.session_id}
          onclick={(event) => event.stopPropagation()}
        >
          {#snippet child({ props })}
            <DropdownMenu.Trigger {...props}>
              <DotsThreeVerticalIcon />
            </DropdownMenu.Trigger>
          {/snippet}
        </Sidebar.MenuAction>
      {/snippet}
      {@render sessionMenuItems(DropdownMenu.Item, DropdownMenu.Separator, session)}
    </ResponsiveMenu>
  </Sidebar.MenuItem>
{/snippet}

<!-- The icon rail has no session list; New Chat follows the Bot avatars instead. -->
{#if sidebar.state === 'collapsed'}
<Sidebar.Group class="pt-0">
  <Sidebar.Separator class="mx-auto mb-2 w-5! data-[orientation=horizontal]:w-5" />
  <Sidebar.Menu>
    <Sidebar.MenuItem>
      <Sidebar.MenuButton isActive={dashboardRelativePath(currentPath) === '/'} tooltipContent={$t('nav.newChat')} onclick={onNewChat}>
        <NotePencilIcon />
        <span>{$t('nav.newChat')}</span>
      </Sidebar.MenuButton>
    </Sidebar.MenuItem>
  </Sidebar.Menu>
</Sidebar.Group>
{/if}

<div class="group-data-[collapsible=icon]:hidden">
  {#if overviewQuery.error}
    <p role="alert" class="px-4 py-2 text-xs text-destructive">{$t('sessions.sidebarFailed')} {overviewQuery.error.message}</p>
  {/if}
  <Sidebar.Group class="pt-3.5">
    <Sidebar.GroupLabel class="sticky top-0 z-10 flex h-8 items-center gap-1 bg-sidebar p-0 pr-0.5 pl-2">
      <button
        type="button"
        class="flex min-w-0 items-center gap-1 rounded-tooltip text-left font-medium hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring focus-visible:outline-hidden"
        aria-expanded={sessionsOpen}
        onclick={() => (sessionsOpen = !sessionsOpen)}
      >
        <span>{$t('sessions.title')}</span>
        <CaretRightIcon class={cn('size-3 transition-transform', sessionsOpen && 'rotate-90')} />
      </button>
      <span class="flex-1"></span>
      <span class="flex items-center gap-0.5">
        <Tooltip.Root>
          <Tooltip.Trigger>
            {#snippet child({ props })}
              <button {...props} type="button" class={headerButtonClass} aria-label={$t('nav.newChat')} onclick={onNewChat}>
                <NotePencilIcon />
              </button>
            {/snippet}
          </Tooltip.Trigger>
          <Tooltip.Content side="bottom">
            {$t('nav.newChat')}
            <ShortcutKeys shortcutId="newChat" aria-hidden="true" />
          </Tooltip.Content>
        </Tooltip.Root>
        <button
          type="button"
          class={headerButtonClass}
          aria-label={$t('sessions.openAll')}
          title={$t('sessions.all')}
          onclick={() => go('/sessions')}
        >
          <ArrowUpRightIcon />
        </button>
        <ResponsiveMenu keepClearOf={filterTrigger} label={$t('sessions.filter')} side="bottom" align="start" class="max-h-96 w-72 overflow-y-auto">
          {#snippet trigger()}
            <DropdownMenu.Trigger bind:ref={filterTrigger}
              class={cn(headerButtonClass, filterApplied && 'text-primary')}
              aria-label={$t('sessions.filter')}
              title={filterTitle}
            >
              <FunnelIcon weight={filterApplied ? 'fill' : 'regular'} />
            </DropdownMenu.Trigger>
          {/snippet}
          <DropdownMenu.Label>{$t('sessions.filterStatus')}</DropdownMenu.Label>
          <DropdownMenu.RadioGroup bind:value={$sessionScope}>
            <DropdownMenu.RadioItem value="active">{$t('sessions.activeOnly')}</DropdownMenu.RadioItem>
            <DropdownMenu.RadioItem value="all">{$t('sessions.all')}</DropdownMenu.RadioItem>
          </DropdownMenu.RadioGroup>
          <DropdownMenu.Separator />
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
        </ResponsiveMenu>
      </span>
    </Sidebar.GroupLabel>
  {#if sessionsOpen}
    <Sidebar.GroupContent class="pr-1">
      <Sidebar.Menu>
        {#each overview.pinned as session}
          {@render sessionMenuItem(session, `pinned:${session.session_id}`)}
        {/each}
        {#if overviewQuery.isPending && !sidebarRecentSessions.length}
          <Sidebar.MenuSkeleton />
          <Sidebar.MenuSkeleton />
        {:else if sidebarRecentSessions.length}
          {#each sidebarRecentSessions as session}
            {@render sessionMenuItem(session, `recent:${session.session_id}`)}
          {/each}
        {:else if !overviewQuery.error}
          <Sidebar.MenuItem>
            <p class="px-2 py-1.5 text-13 leading-5 text-muted-foreground">
              {$sessionScope === 'active' ? $t('sessions.noActive') : $t('sessions.noRecent')}
            </p>
          </Sidebar.MenuItem>
        {/if}
        {#if hasClosedSessions}
          <Sidebar.MenuItem>
            <Sidebar.MenuButton class="text-13 text-muted-foreground touch-target:h-11" onclick={() => sessionScope.set('all')}>
              <ClockCounterClockwiseIcon class="size-3.5 shrink-0" />
              <span>{$t('sessions.showClosed')}</span>
            </Sidebar.MenuButton>
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
