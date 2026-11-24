<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import {
    useSessionPanel,
    type SessionPanelTab,
  } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  import { clientSessionLabel } from '../../../clients'
  import { t } from '$lib/i18n'
  import { statusLabel } from '$lib/i18n/status'
  import { navigate } from '$lib/navigation'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { sessionRecentFiles, withSessionRecentFile } from '$lib/sessionRecentFiles'
  import { compactionDisabledReason } from '$lib/session-chat/compaction'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { isTerminalSession } from '$lib/sessionState'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { useDeviceRuntime } from '../../../device/context'
  import type { SessionView } from '../../../api/types'
  import type { FilePreviewTarget } from '../../../components/chat/filePreviewContext'
  import MobileSessionFilesScreen from './MobileSessionFilesScreen.svelte'
  import MobileSessionGitScreen from './MobileSessionGitScreen.svelte'
  import MobileFilePreviewScreen from './MobileFilePreviewScreen.svelte'
  let {
    session,
    open = $bindable(false),
    pin,
    archive,
    compact,
    exit,
    rename,
    compacting = false,
    fromList = false,
  }: {
    session: SessionView
    open: boolean
    pin: () => void
    archive: () => void
    compact: () => void
    exit: () => void
    rename: () => void
    compacting?: boolean
    fromList?: boolean
  } = $props()
  const device = useDeviceRuntime(),
    stack = useMobileStack()!
  const panel = useSessionPanel()!
  const recent = panel.recentFiles(device)
  let activeLayer: string | undefined
  $effect(() =>
    fromList
      ? undefined
      : panel.attach({ device, sessionId: session.session_id, workspaceId: session.workspace_id })
  )
  function openWorkspace(tab: SessionPanelTab) {
    if (fromList) {
      panel.mobileEntry = { sessionId: session.session_id, tab }
      void navigate('/chat/' + encodeURIComponent(session.session_id))
      return
    }
    panel.tab = tab
    panel.file = null
    panel.git.reset()
    activeLayer = stack.open({
      title: $t(tab === 'files' ? 'files.title' : 'git.title'),
      subtitle: sessionChatTitle(session, $t),
      content: tab === 'files' ? files : git,
    })
  }
  onMount(() => {
    if (!fromList && panel.mobileEntry?.sessionId === session.session_id) {
      const { tab } = panel.mobileEntry
      panel.mobileEntry = undefined
      openWorkspace(tab)
    }
  })
  onDestroy(() => {
    if (!isMobileShell() && stack.layers.some((l) => l.id === activeLayer) && !panel.open) panel.toggle()
  })

  const statuses = device.workspaceQueries.createWorkspaceGitStatusesStore()
  const status = $derived(session.workspace_id ? $statuses[session.workspace_id] : undefined)
  let file = $state.raw<FilePreviewTarget>()
  function remember(target: FilePreviewTarget) {
    panel.file = target
    recent.current = withSessionRecentFile(recent.current, { path: target.path, sessionId: target.sessionId })
  }
  export function openFile(target: FilePreviewTarget) {
    remember(target)
    file = target
    panel.tab = 'files'
    const id = stack.open({ title: target.path, content: preview, dismiss: () => panel.closeFile() })
    if (!stack.layers.some((l) => l.id === activeLayer)) activeLayer = id
  }
</script>

{#snippet preview()}{#if file}<MobileFilePreviewScreen target={file} {remember} />{/if}{/snippet}
{#snippet files()}<MobileSessionFilesScreen
    {recent}
    sessionId={session.session_id}
    onopen={(entry) => openFile({ device, ...entry })}
  />{/snippet}
{#snippet git()}{#if session.workspace_id}<MobileSessionGitScreen
      {device}
      workspaceId={session.workspace_id}
    />{:else}<p>{$t('git.noWorkspaceDescription')}</p>{/if}{/snippet}
<Sheet.Root bind:open>
  <BottomSheet dismiss={() => (open = false)} aria-describedby={undefined}>
    <Sheet.Title class="truncate pr-12 text-lg font-semibold">{sessionChatTitle(session, $t)}</Sheet.Title>
    <p class="truncate pb-2 text-xs text-muted-foreground">
      {statusLabel(session.state, $t)} · {clientSessionLabel(session)} · {session.workspace}
    </p>
    <div class="overflow-auto">
      <button class="mobile-row" onclick={() => stack.back(() => openWorkspace('files'))}
        >{$t('files.title')}<span class="ml-auto"
          >{sessionRecentFiles(recent.current, session.session_id).length}</span
        ></button
      >
      <button class="mobile-row" onclick={() => stack.back(() => openWorkspace('git'))}
        >{$t('git.title')}<span class="ml-auto"
          >{status?.branch ?? ''} {status?.state === 'observed' && !status.clean ? $t('git.dirty') : ''}</span
        ></button
      >
      <button
        class="mobile-row"
        onclick={() => stack.back(() => navigate('/new', { workspace: session.workspace_id }))}
        >{$t('chat.newInWorkspace')}</button
      >
      <button class="mobile-row" onclick={() => stack.back(rename)}>{$t('common.rename')}</button>
      {#if !session.archived_at}<button class="mobile-row" onclick={() => stack.back(pin)}
          >{$t(session.pinned_at ? 'sessions.unpin' : 'sessions.pin')}</button
        >{/if}
      <button class="mobile-row" onclick={() => stack.back(archive)}
        >{$t(session.archived_at ? 'common.unarchive' : 'common.archive')}</button
      >
      <button
        class="mobile-row disabled:opacity-50"
        disabled={!!compactionDisabledReason(session, compacting, $t)}
        onclick={() => stack.back(compact)}>{$t('compaction.action')}</button
      >
      <button
        class="mobile-row text-destructive disabled:opacity-50"
        disabled={isTerminalSession(session)}
        onclick={() => stack.back(exit)}>{$t('chat.endCurrent')}</button
      >
    </div></BottomSheet
  ></Sheet.Root
>
