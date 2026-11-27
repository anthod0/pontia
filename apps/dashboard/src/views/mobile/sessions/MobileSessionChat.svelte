<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import SessionStateDot from './SessionStateDot.svelte'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import { isExecutingSessionState } from '$lib/sessionState'
  import { chromeMetrics } from '$lib/mobile/chromeMetrics'
  import { provideFilePreviewNavigator } from '../../../components/chat/filePreviewContext'
  import DotsThreeIcon from 'phosphor-svelte/lib/DotsThreeIcon'
  import type { SessionChatController } from '../../../features/sessions/sessionChat.svelte'
  import SessionFeed from '../../shared/sessions/SessionFeed.svelte'
  import MobileMessageComposer from '../chat/MobileMessageComposer.svelte'
  import QueuedMessages from '../../../components/chat/QueuedMessages.svelte'
  import RenameSessionDialog from '../../../components/chat/RenameSessionDialog.svelte'
  import MobileSessionActionSheet from './MobileSessionActionSheet.svelte'
  import MobileSessionStepsCard from './MobileSessionStepsCard.svelte'
  import MobileSessionMetaStrip from './MobileSessionMetaStrip.svelte'
  import MobileSessionInteractions from './MobileSessionInteractions.svelte'
  let { chat }: { chat: SessionChatController } = $props()
  const { title: heading, action } = useMobileHeader()
  const session = $derived(chat.selectedSession),
    draft = $derived(chat.chatDraft)
  let menu = $state<MobileSessionActionSheet>(),
    menuOpen = $state(false),
    rename = $state(false),
    height = $state(100)
  provideFilePreviewNavigator({ open: (file) => menu?.openFile(file) })
  onMount(() => {
    heading.set(title)
    action.set(more)
    return () => {
      heading.set(null)
      action.set(null)
    }
  })
</script>

{#snippet title()}
  {#if session}
    <button
      class="mobile-glass flex min-h-11 min-w-0 items-center gap-2 rounded-chip px-4 text-left"
      onclick={() => (menuOpen = true)}
    >
      <strong class="truncate">{sessionChatTitle(session, $t)}</strong>
      <SessionStateDot state={session.state} />
    </button>
  {/if}
{/snippet}
{#snippet more()}{#if session}<button
      class="mobile-icon mobile-glass"
      aria-label={$t('chat.actions')}
      onclick={() => (menuOpen = true)}><DotsThreeIcon weight="bold" /></button
    >{/if}{/snippet}
<SessionFeed {chat} composerHeight={height}>
  {#snippet stepsCard(steps, active, turn)}{#if session}<MobileSessionStepsCard
        {steps}
        {active}
        {turn}
        {session}
      />{/if}{/snippet}
</SessionFeed>
{#if session}
  <div
    class="absolute inset-x-0 bottom-0 z-10 px-3 pb-[max(.5rem,env(safe-area-inset-bottom))]"
    bind:clientHeight={height}
    use:chromeMetrics={'--mobile-footer-h'}
  >
    <MobileSessionInteractions sessionId={session.session_id} />
    <QueuedMessages
      sessionId={session.session_id}
      messages={chat.queuedMessages}
      busyMessageId={chat.queueActionMessageId}
      onCancel={chat.cancelQueueMessage}
    />
    {#if chat.selectedCompaction?.phase === 'failed'}<p role="alert">{chat.selectedCompaction.message}</p>{/if}
    <MobileSessionMetaStrip
      items={chat.selectedSessionMetadataItems}
      refresh={chat.refreshCurrentSessionGitStatus}
    />
    <MobileMessageComposer
      bind:value={$draft}
      draftId={'session:' + session.session_id}
      workspaceId={session.workspace_id ?? undefined}
      commands={chat.commands}
      autofocus={chat.autofocusComposer}
      busy={chat.submitting || chat.actionBusy}
      submitDisabled={!chat.canSend}
      onSubmit={(text) => chat.sendMessage(text, 'immediate')}
      onInterrupt={isExecutingSessionState(session.state) && session.capabilities.interrupt
        ? chat.interruptSelectedSession
        : undefined}
    />
  </div>{/if}
{#if session}<MobileSessionActionSheet
    bind:this={menu}
    {session}
    bind:open={menuOpen}
    pin={() => chat.toggleSelectedSessionManagement('/pin')}
    archive={() => chat.toggleSelectedSessionManagement('/archive')}
    exit={chat.exitSelectedSession}
    compact={chat.compactSelectedSession}
    compacting={chat.selectedCompaction?.busy}
    rename={() => (rename = true)}
  />
  <RenameSessionDialog
    {session}
    bind:open={rename}
    busy={chat.actionBusy}
    error={chat.actionError}
    onConfirm={async (title) => {
      await chat.renameSelectedSession(title)
      if (!chat.actionError) rename = false
    }}
  />{/if}
