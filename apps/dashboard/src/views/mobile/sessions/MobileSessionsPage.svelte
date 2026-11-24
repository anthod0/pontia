<script lang="ts">
  import { toast } from 'svelte-sonner'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { compactionDisabledReason } from '$lib/session-chat/compaction'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { useDeviceRuntime } from '../../../device/context'
  import type { SessionView } from '../../../api/types'
  import { createSessionList, type SessionScope } from '../../../features/sessions/sessionList.svelte'
  import {
    useSessionPanel,
    type SessionPanelTab,
  } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import RenameSessionDialog from '../../../components/chat/RenameSessionDialog.svelte'
  import MobileWorkspaceSheet from '../chat/MobileWorkspaceSheet.svelte'
  import MobileSessionFilterBar from './MobileSessionFilterBar.svelte'
  import MobileSessionRow from './MobileSessionRow.svelte'
  import { workspaceTitle } from '../../../components/chat/sessionMetadata'
  const device = useDeviceRuntime(),
    stack = useMobileStack()!,
    panel = useSessionPanel()
  const { selectedWorkspaceId, sessionScope } = device.sidebarSessionFilter
  let specialScope = $state<'pinned' | 'archived' | null>(null)
  const scope = $derived(specialScope ?? $sessionScope)
  const workspace = $derived($selectedWorkspaceId)
  let reveal = $state<string | null>(null),
    workspaceOpen = $state(false),
    menu = $state<SessionView | null>(null)
  let rename = $state<SessionView | null>(null),
    renameOpen = $state(false),
    renameError = $state('')
  const workspaces = device.workspaceQueries.createWorkspacesQuery()
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
      toast.error(e instanceof Error ? e.message : String(e))
    }
  }
  function openPanel(s: SessionView, tab: SessionPanelTab) {
    if (panel) {
      panel.tab = tab
      if (!panel.open) panel.toggle()
    }
    void navigate('/chat/' + encodeURIComponent(s.session_id))
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
</script>

<MobileSessionFilterBar
  {scope}
  select={filter}
  workspace={current ? workspaceTitle(current) : ''}
  open={() => (workspaceOpen = true)}
/>
<div class="pb-16">
  {#if sessions.query.error}<p role="alert">{sessions.query.error.message}</p>{/if}
  {#each sessions.list as session, i (session.session_id)}
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
      }}
      busy={sessions.busyId === session.session_id}
      pin={() => run(() => sessions.pin(session))}
      archive={() => run(() => sessions.archive(session))}
    />
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
</div>
<button
  class="absolute right-4 bottom-[calc(var(--mobile-tab-h)+var(--mobile-tab-gap)+14px)] flex h-13 items-center gap-2 rounded-2xl bg-primary px-5 text-primary-foreground shadow-md"
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
  <Sheet.Root
    open
    onOpenChange={(v) => {
      if (!v) menu = null
    }}
  >
    <BottomSheet dismiss={() => (menu = null)} aria-describedby={undefined}>
      <Sheet.Title class="truncate py-3 pr-12 text-lg font-semibold">{sessionChatTitle(s, $t)}</Sheet.Title>
      <div class="overflow-auto">
        <button class="mobile-row" onclick={() => stack.back(() => openPanel(s, 'files'))}
          >{$t('files.title')}</button
        >
        <button class="mobile-row" onclick={() => stack.back(() => openPanel(s, 'git'))}>{$t('git.title')}</button>
        <button
          class="mobile-row"
          onclick={() => stack.back(() => navigate('/new', { workspace: s.workspace_id }))}
          >{$t('chat.newInWorkspace')}</button
        >
        <button
          class="mobile-row"
          onclick={() =>
            stack.back(() => {
              rename = s
              renameError = ''
              renameOpen = true
            })}>{$t('common.rename')}</button
        >
        {#if !s.archived_at}<button class="mobile-row" onclick={() => stack.back(() => run(() => sessions.pin(s)))}
            >{$t(s.pinned_at ? 'sessions.unpin' : 'sessions.pin')}</button
          >{/if}
        <button class="mobile-row" onclick={() => stack.back(() => run(() => sessions.archive(s)))}
          >{$t(s.archived_at ? 'common.unarchive' : 'common.archive')}</button
        >
        <button
          class="mobile-row disabled:opacity-50"
          disabled={!!compactionDisabledReason(s, false, $t)}
          onclick={() => stack.back(() => run(() => device.api.compactSession(s.session_id)))}
          >{$t('compaction.action')}</button
        >
        <button
          class="mobile-row text-destructive disabled:opacity-50"
          disabled={s.state === 'exited' || s.state === 'error'}
          onclick={() => stack.back(() => run(() => sessions.exit(s)))}>{$t('chat.endCurrent')}</button
        >
      </div>
    </BottomSheet>
  </Sheet.Root>
{/if}
<RenameSessionDialog
  bind:open={renameOpen}
  session={rename}
  busy={!!sessions.busyId}
  error={renameError}
  onConfirm={save}
/>
