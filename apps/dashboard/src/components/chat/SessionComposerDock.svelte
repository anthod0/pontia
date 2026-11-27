<script lang="ts">
  import { t } from '$lib/i18n';
  import type { MessageMode, QueuedMessageView, SessionView, WorkspaceGitStatusView } from '../../api/types'
  import { canSendSessionMessage } from '$lib/session-chat/sessionChat'
  import MessageComposer from './MessageComposer.svelte'
  import QueuedMessages from './QueuedMessages.svelte'
  import SessionMetadata from './SessionMetadata.svelte'
  import { type SessionMetadataItem } from './sessionMetadata'
  import type { ChatCommand } from '$lib/chatCommands'
  import { isExecutingSessionState } from '$lib/sessionState'
  import { type compactionStatus } from '$lib/session-chat/compaction'

  interface Props {
    commands: ChatCommand[]
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    metadataItems: SessionMetadataItem[]
    metadataSummary: string
    queuedMessages: QueuedMessageView[]
    queueBusyMessageId: string | null
    input: string
    height?: number
    submitting?: boolean
    actionBusy?: boolean
    compaction?: ReturnType<typeof compactionStatus>
    canSend?: boolean
    autofocus?: boolean
    onCancelQueuedMessage: (message: QueuedMessageView) => void
    onSend: (message: string, mode: MessageMode) => Promise<boolean>
    onInterrupt: () => void
    onFocus: () => void
    onRefreshGitStatus: () => void
  }

  let {
    commands,
    session,
    gitStatus,
    metadataItems,
    metadataSummary,
    queuedMessages,
    queueBusyMessageId,
    input = $bindable(''),
    height = $bindable(0),
    submitting = false,
    actionBusy = false,
    compaction = null,
    canSend = false,
    autofocus = false,
    onCancelQueuedMessage,
    onSend,
    onInterrupt,
    onFocus,
    onRefreshGitStatus,
  }: Props = $props()

  let composerDisabled = $derived(submitting || actionBusy)
  let interruptMode = $derived(isExecutingSessionState(session.state) && session.capabilities?.interrupt === true && input.trim() === '')
</script>

<div bind:clientHeight={height} data-chat-composer-dock="fixed" class="fixed bottom-0 left-0 right-0 lg:right-[var(--file-preview-width,0px)] z-30 bg-surface px-4 pb-[max(0.5rem,env(safe-area-inset-bottom))] md:pb-[max(1.25rem,env(safe-area-inset-bottom))] md:left-[var(--sidebar-width)] md:px-8 transition-[left] duration-200 ease-linear group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]">
  <div aria-hidden="true" class="pointer-events-none absolute inset-x-0 bottom-full h-6 bg-linear-to-t from-surface to-surface/0 backdrop-blur-sm mask-t-from-0%"></div>
  <div class="mx-auto w-full max-w-[760px]">
    {#if compaction?.phase === 'failed'}
      <p role="alert" class="mb-2 text-xs text-destructive">{compaction.message}</p>
    {/if}
    <QueuedMessages
      sessionId={session.session_id}
      messages={queuedMessages}
      busyMessageId={queueBusyMessageId}
      onCancel={onCancelQueuedMessage}
    />
    <MessageComposer draftId={`session:${session.session_id}`} bind:value={input} {commands} workspaceId={session.workspace_id} busy={submitting} disabled={composerDisabled} submitDisabled={!canSend} fullscreen {autofocus} {interruptMode} interruptBusy={actionBusy} onSubmit={onSend} {onInterrupt} {onFocus}>
      {#snippet header()}
        <SessionMetadata {session} {gitStatus} {metadataItems} {metadataSummary} {onRefreshGitStatus} />
      {/snippet}
    </MessageComposer>
    {#if canSendSessionMessage(session, 'x') === false}
      <p class="mt-2 text-xs text-muted-foreground">{$t('chat.cannotAcceptMessages')}</p>
    {/if}
  </div>
</div>
