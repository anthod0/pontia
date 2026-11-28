<script lang="ts">
  import { toast } from 'svelte-sonner'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import * as Tabs from '$lib/components/ui/tabs'
  import * as ContextMenu from '$lib/components/ui/context-menu'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { sessionRecentFiles } from '$lib/sessionRecentFiles'
  import { useDeviceRuntime } from '../../../device/context'
  import type { SessionView } from '../../../api/types'
  import { createSessionList, type SessionScope } from '../../../features/sessions/sessionList.svelte'
  import MobileSessionActionSheet from './MobileSessionActionSheet.svelte'
  import RenameSessionDialog from '../../../components/chat/RenameSessionDialog.svelte'
  import MobileWorkspaceSheet from '../chat/MobileWorkspaceSheet.svelte'
  import MobileSessionFilterBar from './MobileSessionFilterBar.svelte'
  import MobileSessionRow from './MobileSessionRow.svelte'
  import MobileSessionActionItems from './MobileSessionActionItems.svelte'
  import { useSessionPanel, type SessionPanelTab } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { workspaceTitle } from '../../../components/chat/sessionMetadata'
  const device = useDeviceRuntime()
  const { selectedWorkspaceId, sessionScope } = device.sidebarSessionFilter
  let specialScope = $state<'pinned' | 'archived' | null>(null)
  const scope = $derived(specialScope ?? $sessionScope)
  const workspace = $derived($selectedWorkspaceId)
  let reveal = $state<string | null>(null),
    workspaceOpen = $state(false),
    menu = $state<SessionView | null>(null)
  let menuOpen = $state(false)
  let rename = $state<SessionView | null>(null),
    renameOpen = $state(false),
    renameError = $state('')
  const workspaces = device.workspaceQueries.createWorkspacesQuery()
  const panel = useSessionPanel()!
  const recent = panel.recentFiles(device)
  const statuses = device.workspaceQueries.createWorkspaceGitStatusesStore()
  const current = $derived(workspaces.data?.find((w) => w.workspace_id === workspace))
  const sessions = createSessionList(
    device,
    true,
    () => workspace,
    () => scope,
  )
  function filter(s: SessionScope) {
    if (s === 'all' || s === 'active') {
      sessionScope.set(s)
      specialScope = null
    } else specialScope = s
    reveal = null
  }
  async function run(action: () => Promise<unknown>) {
    reveal = null
    try {
      await action()
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e), { duration: 6000 })
    }
  }
  async function save(title: string | null) {
    if (!rename) return
    try {
      await sessions.rename(rename, title)
      renameOpen = false
    } catch (e) {
      renameError = e instanceof Error ? e.message : String(e)
    }
  }
  function openWorkspace(session: SessionView, tab: SessionPanelTab) {
    panel.mobileEntry = { sessionId: session.session_id, tab }
    void navigate('/chat/' + encodeURIComponent(session.session_id))
  }
</script>

<Tabs.Root value={scope} onValueChange={(value) => filter(value as SessionScope)} class="gap-1">
  <MobileSessionFilterBar
    workspace={current ? workspaceTitle(current) : ''}
    open={() => (workspaceOpen = true)}
  />
  <Tabs.Content value={scope} class="pb-16 text-base">
    {#if sessions.query.error}<p role="alert">{sessions.query.error.message}</p>{/if}
    {#each sessions.list as session, i (session.session_id)}
      {@const status = session.workspace_id ? $statuses[session.workspace_id] : undefined}
      {#if scope !== 'pinned' && session.pinned_at && i === 0}<h2 class="py-2 text-sm text-muted-foreground">
          {$t('common.pinned')}
        </h2>
      {:else if i > 0 && sessions.list[i - 1].pinned_at && !session.pinned_at}<h2
          class="py-2 text-sm text-muted-foreground"
        >
          {$t('sessions.recent')}
        </h2>{/if}
      <MobileSessionRow
        {session}
        open={reveal === session.session_id}
        reveal={(v) => (reveal = v ? session.session_id : null)}
        menu={() => {
          reveal = null
          menu = session
          menuOpen = true
        }}
        busy={sessions.busyId === session.session_id}
        pin={() => run(() => sessions.pin(session))}
        archive={() => run(() => sessions.archive(session))}
      >
        {#snippet contextMenu()}
          <MobileSessionActionItems
            {session}
            fileCount={sessionRecentFiles(recent.current, session.session_id).length}
            gitSummary={`${status?.branch ?? ''} ${status?.state === 'observed' && !status.clean ? $t('git.dirty') : ''}`.trim()}
            openFiles={() => openWorkspace(session, 'files')}
            openGit={() => openWorkspace(session, 'git')}
            newSession={() => void navigate('/new', { workspace: session.workspace_id })}
            rename={() => { rename = session; renameError = ''; renameOpen = true }}
            pin={() => void run(() => sessions.pin(session))}
            archive={() => void run(() => sessions.archive(session))}
            compact={() => void run(() => device.api.compactSession(session.session_id))}
            exit={() => void run(() => sessions.exit(session))}
            busy={sessions.busyId === session.session_id}
          >
            {#snippet item(action, children)}
              <ContextMenu.Item disabled={action.disabled} variant={action.destructive ? 'destructive' : 'default'} onSelect={action.select}>
                {@render children()}
              </ContextMenu.Item>
            {/snippet}
          </MobileSessionActionItems>
        {/snippet}
      </MobileSessionRow>
    {:else}{#if !sessions.query.error}
        <p role="status" class="py-8 text-center text-muted-foreground">
          {$t(
            sessions.query.isPending
              ? 'common.loading'
              : scope === 'archived'
                ? 'sessions.noArchived'
                : scope === 'active'
                  ? 'sessions.noActive'
                  : scope === 'pinned'
                    ? 'sessions.noPinned'
                    : 'sessions.empty',
          )}
        </p>{/if}
    {/each}
    {#if scope === 'all' && sessions.query.hasNextPage}<button
        class="mobile-row"
        disabled={sessions.query.isFetchingNextPage}
        onclick={sessions.loadNextPage}>{$t('common.loadMore')}</button
      >{/if}
    {#if scope === 'active'}<button
        class="min-h-11 w-full text-center text-sm text-primary"
        onclick={() => filter('all')}>{$t('sessions.showClosed')}</button
      >{/if}
  </Tabs.Content>
</Tabs.Root>
<button
  data-mobile-toast-avoid
  class="absolute right-4 bottom-[calc(var(--mobile-tab-h)+var(--mobile-tab-gap)+14px)] flex h-13 items-center gap-2 rounded-panel bg-primary px-5 text-primary-foreground shadow-md dark:text-background"
  onclick={() => navigate('/new', { workspace: workspace || null })}><PlusIcon />{$t('nav.newChat')}</button
>
{#if workspaceOpen}<MobileWorkspaceSheet
    workspaces={workspaces.data ?? []}
    value={workspace}
    allowAll
    choose={(id) => {
      selectedWorkspaceId.set(id)
      reveal = null
    }}
    close={() => (workspaceOpen = false)}
  />{/if}
{#if menu}
  {@const s = menu}
  <MobileSessionActionSheet fromList session={s} bind:open={menuOpen}
    pin={() => run(() => sessions.pin(s))} archive={() => run(() => sessions.archive(s))}
    compact={() => run(() => device.api.compactSession(s.session_id))} exit={() => run(() => sessions.exit(s))}
    rename={() => {rename=s; renameError=''; renameOpen=true}} />
{/if}
<RenameSessionDialog
  bind:open={renameOpen}
  session={rename}
  busy={!!sessions.busyId}
  error={renameError}
  onConfirm={save}
/>
