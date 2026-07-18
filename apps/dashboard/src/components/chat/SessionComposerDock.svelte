<script lang="ts">
  import type { SessionView, WorkspaceGitStatusView, WorkspaceView } from '../../api/types'
  import { canSendSessionMessage } from '$lib/session-chat/sessionChat'
  import MessageComposer from './MessageComposer.svelte'
  import SessionActions from './SessionActions.svelte'
  import SessionMetadata from './SessionMetadata.svelte'
  import { type SessionMetadataItem } from './sessionMetadata'

  interface Props {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    workspaces: WorkspaceView[]
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
    inboxActionableCount: number
    input: string
    submitting?: boolean
    actionBusy?: boolean
    canSend?: boolean
    onOpenInbox: () => void
    onExit: () => void
    onOpenConsole: () => void
    onNewChat: () => void
    onRename: () => void
    onRestart: () => void
    onSend: () => void
    onFocus: () => void
  }

  let {
    session,
    gitStatus,
    workspaces,
    metadataItems,
    metadataSummary,
    inboxActionableCount,
    input = $bindable(''),
    submitting = false,
    actionBusy = false,
    canSend = false,
    onOpenInbox,
    onExit,
    onOpenConsole,
    onNewChat,
    onRename,
    onRestart,
    onSend,
    onFocus,
  }: Props = $props()

  let canAcceptWebInput = $derived(session.capabilities?.accept_task === true)
  let composerDisabled = $derived(!canAcceptWebInput || session.state === 'error' || submitting)
</script>

<div data-chat-composer-dock="fixed" class="fixed bottom-0 left-0 right-0 z-30 bg-surface px-2 pb-2 pt-1 sm:px-4 md:left-[var(--sidebar-width)] md:px-6 md:pb-3 transition-[left] duration-200 ease-linear group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]">
  <div aria-hidden="true" data-chat-composer-fade="outward" class="pointer-events-none absolute inset-x-0 bottom-full h-12 bg-gradient-to-t from-surface via-surface/70 to-transparent"></div>
  <div class="mx-auto w-full max-w-4xl">
    <div role="group" aria-label="Session status and controls" class="mb-1 flex min-w-0 items-center justify-between gap-2 px-2">
      <div class="flex min-w-0 flex-1 items-center gap-2">
        <SessionMetadata {session} {gitStatus} {workspaces} {metadataItems} {metadataSummary} />
      </div>
      <div class="flex shrink-0 items-center justify-end gap-2">
        <SessionActions {session} {inboxActionableCount} {actionBusy} {onOpenInbox} {onExit} {onOpenConsole} {onNewChat} {onRename} {onRestart} />
      </div>
    </div>
    <MessageComposer bind:value={input} workspaceId={session.workspace_id} busy={submitting} disabled={composerDisabled} submitDisabled={!canSend} fullscreen onSubmit={onSend} {onFocus} />
    {#if !canAcceptWebInput}
      <p class="mt-2 text-xs text-muted-foreground">此 session 当前不可从 Web 写入</p>
    {:else if canSendSessionMessage(session, 'x') === false}
      <p class="mt-2 text-xs text-muted-foreground">This session cannot accept new messages.</p>
    {/if}
  </div>
</div>
