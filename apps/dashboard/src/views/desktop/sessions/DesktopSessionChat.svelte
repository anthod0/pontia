<script lang="ts">
  import ChatRuler from '$lib/components/session-chat/ChatRuler.svelte'
  import SessionComposerDock from '../../../components/chat/SessionComposerDock.svelte'
  import SessionPanelSession from '../../../components/chat/sessionPanel/SessionPanelSession.svelte'
  import SessionInteractions from '../../../components/chat/SessionInteractions.svelte'
  import type { SessionChatController } from '../../../features/sessions/sessionChat.svelte'
  import SessionFeed from '../../shared/sessions/SessionFeed.svelte'
  import { provideLocalhostPreview } from '../../../components/chat/localhostPreviewContext'
  import { useSessionPanel } from '../../../components/chat/sessionPanel/sessionPanelContext.svelte'
  const panel = useSessionPanel()
  if (panel) provideLocalhostPreview(target => panel.openPreview(target))
  let composerHeight = $state(180)
  let { chat }: { chat: SessionChatController } = $props()
  const chatDraft = $derived(chat.chatDraft)
  const selectedSession = $derived(chat.selectedSession)
</script>

{#if selectedSession}
  <SessionPanelSession sessionId={selectedSession.session_id} workspaceId={selectedSession.workspace_id} />
{/if}

<SessionFeed {chat} {composerHeight}>
  {#snippet interactions()}<SessionInteractions sessionId={chat.selectedSessionId} />{/snippet}
  {#snippet dock()}
    {#if selectedSession}
      <SessionComposerDock
        commands={chat.commands}
        bind:height={composerHeight}
        bind:input={$chatDraft}
        session={selectedSession}
        gitStatus={chat.selectedSessionGitStatus}
        metadataItems={chat.selectedSessionMetadataItems}
        metadataSummary={chat.selectedSessionMetadataSummary}
        queuedMessages={chat.visibleInboxMessages}
        inboxBusyMessageId={chat.inboxActionMessageId ?? chat.pendingInboxSubmissionId}
        submitting={chat.submitting}
        actionBusy={chat.actionBusy}
        compaction={chat.selectedCompaction}
        canSend={chat.canSend}
        autofocus={chat.autofocusComposer}
        onCancelInboxMessage={(message) => void chat.cancelPendingInboxMessage(message)}
        onRetryInboxMessage={(message) => void chat.retryFailedInboxMessage(message)}
        onDismissInboxMessage={(message) => void chat.dismissFailedInboxMessage(message)}
        onSend={chat.sendMessage}
        onInterrupt={() => void chat.interruptSelectedSession()}
        onFocus={() => void chat.refreshCurrentSessionGitStatus()}
        onRefreshGitStatus={() => void chat.refreshCurrentSessionGitStatus()}
      />
    {/if}
  {/snippet}
</SessionFeed>
{#if selectedSession && !chat.initialChatScrollPending && !chat.transcriptResolving && chat.rulerTurns.length}
  <ChatRuler
    turns={chat.rulerTurns}
    treeMode={chat.rulerTreeMode}
    navigableTurnIds={chat.rulerNavigableTurnIds}
    onNavigate={chat.navigateFromRuler}
  />
{/if}
