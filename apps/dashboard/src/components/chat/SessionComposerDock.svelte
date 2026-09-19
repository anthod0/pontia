<script lang="ts">
  import type { InboxMessageView, SessionView, WorkspaceGitStatusView } from '../../api/types'
  import { canSendSessionMessage } from '$lib/session-chat/sessionChat'
  import MessageComposer from './MessageComposer.svelte'
  import QueuedMessages from './QueuedMessages.svelte'
  import SessionMetadata from './SessionMetadata.svelte'
  import { type SessionMetadataItem } from './sessionMetadata'

  interface Props {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
    queuedMessages: InboxMessageView[]
    inboxBusyMessageId: string | null
    input: string
    height?: number
    submitting?: boolean
    actionBusy?: boolean
    canSend?: boolean
    autofocus?: boolean
    onCancelInboxMessage: (message: InboxMessageView) => void
    onRetryInboxMessage: (message: InboxMessageView) => void
    onDismissInboxMessage: (message: InboxMessageView) => void
    onSend: () => void
    onInterrupt: () => void
    onFocus: () => void
  }

  let {
    session,
    gitStatus,
    metadataItems,
    metadataSummary,
    queuedMessages,
    inboxBusyMessageId,
    input = $bindable(''),
    height = $bindable(0),
    submitting = false,
    actionBusy = false,
    canSend = false,
    autofocus = false,
    onCancelInboxMessage,
    onRetryInboxMessage,
    onDismissInboxMessage,
    onSend,
    onInterrupt,
    onFocus,
  }: Props = $props()

  let canAcceptWebInput = $derived(session.capabilities?.accept_task === true)
  let composerDisabled = $derived(!canAcceptWebInput || session.state === 'error' || submitting)
  let interruptMode = $derived(session.state === 'busy' && session.capabilities?.interrupt === true && input.trim() === '')
</script>

<div bind:clientHeight={height} data-chat-composer-dock="fixed" class="fixed bottom-0 left-0 right-0 z-30 bg-surface px-4 pb-[max(1.25rem,env(safe-area-inset-bottom))] pt-2 md:left-[var(--sidebar-width)] md:px-8 transition-[left] duration-200 ease-linear group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]">
  <div class="mx-auto w-full max-w-[760px]">
    <QueuedMessages
      messages={queuedMessages}
      busyMessageId={inboxBusyMessageId}
      onCancel={onCancelInboxMessage}
      onRetry={onRetryInboxMessage}
      onDismiss={onDismissInboxMessage}
    />
    <div class="mb-2 flex min-w-0 items-center gap-2">
      <SessionMetadata {gitStatus} {metadataItems} {metadataSummary} />
    </div>
    <MessageComposer bind:value={input} workspaceId={session.workspace_id} busy={submitting} disabled={composerDisabled} submitDisabled={!canSend} fullscreen {autofocus} {interruptMode} interruptBusy={actionBusy} onSubmit={onSend} {onInterrupt} {onFocus} />
    {#if canSendSessionMessage(session, 'x') === false}
      <p class="mt-2 text-xs text-muted-foreground">This session cannot accept new messages.</p>
    {/if}
  </div>
</div>
