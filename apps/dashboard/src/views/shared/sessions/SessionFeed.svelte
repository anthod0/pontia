<script lang="ts">
  import { t } from '$lib/i18n'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import SessionConversationSkeleton from '$lib/components/session-chat/SessionConversationSkeleton.svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import SessionConversation from '$lib/components/session-chat/SessionConversation.svelte'
  import AgentStatus from '$lib/components/session-chat/AgentStatus.svelte'
  import ScrollToBottomButton from '../../../components/chat/ScrollToBottomButton.svelte'
  import type { SessionChatController } from '../../../features/sessions/sessionChat.svelte'
  import type { Snippet, ComponentProps } from 'svelte'
  let bottomSentinel = $state<HTMLElement | null>(null)
  let scrollDownButton = $state<ScrollToBottomButton>()
  $effect(() => {
    const button = scrollDownButton
    chat.setHideScrollButton(() => button?.hide())
    return () => chat.setHideScrollButton()
  })
  let {
    chat,
    composerHeight,
    dock,
    interactions,
    stepsCard,
  }: {
    chat: SessionChatController
    composerHeight: number
    dock?: Snippet
    interactions?: Snippet
    stepsCard?: ComponentProps<typeof SessionConversation>['stepsCard']
  } = $props()
  const pendingHistoryForks = $derived(chat.pendingHistoryForks)
  const selectedSession = $derived(chat.selectedSession)
  const sessionDetailError = $derived(chat.sessionDetailError)
  const sessionDetailErrorKind = $derived(chat.sessionDetailErrorKind)
  const sessionDetailLoading = $derived(chat.sessionDetailLoading)
  const timelineState = $derived(chat.timelineState)
</script>

<section
  class="flex flex-col gap-4 pb-[var(--chat-bottom-padding)]"
  style={`--chat-top-offset: 4rem; --chat-bottom-padding: ${composerHeight + 16}px; --chat-composer-height: ${composerHeight}px`}
>
  {#if selectedSession?.control_connected === false}
    <p class="text-sm text-destructive" role="status">
      {$t(
        selectedSession.control_disconnected_reason === 'unsupported_protocol_version'
          ? 'chat.controlProtocolMismatch'
          : 'chat.clientDisconnected'
      )}
    </p>
  {/if}
  {#if chat.controlDetails?.diagnostic}
    <Alert.Root
      variant={chat.controlDetails.diagnostic.kind === 'error' ? 'destructive' : 'default'}
      role={chat.controlDetails.diagnostic.kind === 'error' ? 'alert' : 'status'}
      class="mx-auto w-full max-w-[760px]"
    >
      <WarningCircleIcon class="size-4" />
      <Alert.Title>
        {$t(chat.controlDetails.diagnostic.kind === 'error' ? 'chat.clientConnectionFailed' : 'chat.clientReconnecting')}
      </Alert.Title>
      {#if chat.controlDetails.diagnostic.message}
        <Alert.Description>{chat.controlDetails.diagnostic.message}</Alert.Description>
      {/if}
    </Alert.Root>
  {/if}
  {#if selectedSession && chat.controlDetails && (selectedSession.capabilities.accept_task || chat.controlDetails.unavailableReason) && (chat.controlDetails.unavailableReason || chat.controlDetails.profile?.error || !chat.sessionSupportsTimeline(selectedSession))}
    <div class="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
      {#if chat.controlDetails.unavailableReason}
        <span class="text-destructive" role="alert">{chat.controlDetails.unavailableReason}</span>
      {/if}
      {#if chat.controlDetails.profile?.error}
        <span class="text-destructive" role="alert">{chat.controlDetails.profile.error}</span>
      {/if}
      {#if !chat.sessionSupportsTimeline(selectedSession)}<span>{$t('chat.historyUnavailable')}</span>{/if}
    </div>
  {/if}
  {#if $sessionDetailError && selectedSession}
    <Alert.Root variant="destructive" role="alert" class="mx-auto w-full max-w-[760px]">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('chat.refreshFailed')}</Alert.Title>
      <Alert.Description>{$sessionDetailError} {$t('chat.lastSnapshot')}</Alert.Description>
    </Alert.Root>
  {/if}
  {#if chat.actionError}
    <Alert.Root variant="destructive" role="alert" class="mx-auto w-full max-w-[760px]">
      <WarningCircleIcon class="size-4" />
      <Alert.Title>{$t('chat.actionFailed')}</Alert.Title>
      <Alert.Description>{chat.actionError}</Alert.Description>
    </Alert.Root>
  {/if}
  <div class="mx-auto min-w-0 w-full max-w-[760px] flex-1">
    <div class="relative flex min-w-0 flex-col rounded-none bg-transparent">
      {#if $sessionDetailLoading && !selectedSession}
        <SessionConversationSkeleton class="min-h-80" />
      {:else if !selectedSession && $sessionDetailErrorKind !== 'not_found'}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Title
              >{$sessionDetailErrorKind === 'authentication'
                ? $t('auth.required')
                : $sessionDetailErrorKind === 'network'
                  ? $t('connection.failed')
                  : $t('chat.loadFailed')}</Empty.Title
            >
            <Empty.Description
              >{$sessionDetailError ?? $t('chat.loading')} {$t('chat.reloadOnReconnect')}</Empty.Description
            >
          </Empty.Header>
          <Empty.Content
            ><Button
              onclick={() => void chat.loadSelectedSession(chat.selectedSessionId)}
              disabled={$sessionDetailLoading}>{$t('common.retry')}</Button
            ></Empty.Content
          >
        </Empty.Root>
      {:else if !selectedSession}
        <Empty.Root class="h-full">
          <Empty.Header>
            <Empty.Title>{$t('chat.notFound')}</Empty.Title>
            <Empty.Description>{$t('chat.chooseSession')}</Empty.Description>
          </Empty.Header>
          <Empty.Content><Button onclick={() => chat.openNewChat()}>{$t('chat.startNew')}</Button></Empty.Content>
        </Empty.Root>
      {:else}
        <div
          data-chat-initial-scroll-pending={chat.initialChatScrollPending || chat.transcriptResolving
            ? 'true'
            : 'false'}
          class={chat.initialChatScrollPending || chat.transcriptResolving ? 'relative min-h-80' : 'relative'}
        >
          {#if chat.initialChatScrollPending}
            <SessionConversationSkeleton class="pointer-events-none absolute inset-x-0 top-0 z-10" />
          {/if}
          <div class={chat.initialChatScrollPending || chat.transcriptResolving ? 'opacity-0' : ''}>
            {#if !chat.sessionSupportsTimeline(selectedSession) || chat.timelineUnavailable}
              {#if selectedSession.state === 'waiting_for_interaction'}
                <AgentStatus state={selectedSession.state} />
              {/if}
              <Empty.Root data-timeline-status={$timelineState.status} class="min-h-80">
                <Empty.Header>
                  <Empty.Title
                    >{$timelineState.status === 'pending'
                      ? $t('chat.waitingHistory')
                      : $t('chat.conversationUnavailable')}</Empty.Title
                  >
                  <Empty.Description
                    >{!chat.sessionSupportsTimeline(selectedSession)
                      ? (selectedSession.timeline_unavailable_reason ?? $t('chat.noConversationHistory'))
                      : $timelineState.error}</Empty.Description
                  >
                </Empty.Header>
                <Empty.Content>
                  <Button variant="outline" onclick={() => chat.loadSelectedSession(chat.selectedSessionId)}
                    >{$t('chat.retryHistory')}</Button
                  >
                </Empty.Content>
              </Empty.Root>
            {:else}
              {#if chat.compactedHistoryNotice}
                <p role="status" class="py-2 text-sm text-muted-foreground">{$t('compaction.earlierHistory')}</p>
              {/if}
              {#if $timelineState.status === 'pending'}
                <p role="status" class="py-2 text-sm text-muted-foreground">{$t('chat.loadingHistory')}</p>
              {/if}
              {#if $timelineState.compactionHistoryError}
                <Alert.Root class="my-3" role="status">
                  <WarningCircleIcon class="size-4" />
                  <Alert.Title>{$t('chat.compactionHistoryUnavailable')}</Alert.Title>
                  <Alert.Description>
                    {$timelineState.compactionHistoryError}
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={$timelineState.refreshing}
                      onclick={() => void chat.refreshSessionTimeline(chat.selectedSessionId)}
                      >{$t('common.retry')}</Button
                    >
                  </Alert.Description>
                </Alert.Root>
              {/if}
              {#each $timelineState.groups.filter((group) => group.history_issue) as group (group.turn_id)}
                <Alert.Root class="my-3" role="status">
                  <WarningCircleIcon class="size-4" />
                  <Alert.Title>{$t('chat.historyUnassociated')}</Alert.Title>
                  <Alert.Description>
                    {#if group.history_issue === 'topology_unknown'}
                      {$t('chat.branchUnconfirmed')}
                    {:else}
                      {$t('chat.unconfirmedRange')}
                    {/if}
                  </Alert.Description>
                </Alert.Root>
              {/each}
            {/if}
            {#if (chat.sessionSupportsTimeline(selectedSession) && !chat.timelineUnavailable) || (!chat.sessionSupportsTimeline(selectedSession) && (selectedSession.capabilities.stream_output || Object.keys(chat.liveOutputOverlays).length > 0))}
              {#key chat.selectedSessionId}
                <SessionConversation
                  {stepsCard}
                  messages={chat.messages}
                  timelineItems={chat.conversationTimelineItems}
                  compactionBusy={chat.selectedCompaction?.busy ?? false}
                  sessionId={chat.selectedSessionId}
                  workspace={selectedSession.workspace}
                  turns={chat.selectedTurns}
                  sessionState={selectedSession.state}
                  activeTurnId={selectedSession.current_turn_id}
                  loading={(chat.initialChatScrollPending ||
                    chat.transcriptResolving ||
                    $sessionDetailLoading ||
                    $timelineState.loading) &&
                    !chat.messages.length}
                  hasMoreHistory={$timelineState.hasMore}
                  historyLoading={$timelineState.refreshKind === 'history'}
                  historyObserverEnabled={chat.historyObserverEnabled}
                  branchActionInputs={chat.branchActionInputs}
                  branchActionBusy={chat.branchActionSubmitting}
                  onBranchEdit={chat.editHistoricalMessage}
                  onLoadMoreHistory={chat.loadEarlierMessages}
                />
              {/key}
            {/if}
          </div>
        </div>
        {@render interactions?.()}
        {#each $pendingHistoryForks.filter((operation) => operation.sourceSessionId === chat.selectedSessionId || operation.sessionId === chat.selectedSessionId) as operation (operation.operationId)}
          <Alert.Root variant="destructive" class="mx-4 my-2">
            <WarningCircleIcon class="size-4" />
            <Alert.Title
              >{operation.stage === 'fork_failed'
                ? $t('chat.forkFailed')
                : operation.stage === 'session_pending'
                  ? $t('chat.forkPending')
                  : operation.stage === 'input_unconfirmed'
                    ? $t('chat.submissionUnconfirmed')
                    : $t('chat.forkUnconfirmed')}</Alert.Title
            >
            <Alert.Description>
              {operation.error ?? $t('chat.recoverProgress')}
              <div class="mt-2 flex gap-2">
                {#if operation.stage === 'fork_failed'}
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => chat.device.historyForks.forget(operation.operationId)}
                    >{$t('common.dismiss')}</Button
                  >
                {:else}
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={chat.branchActionSubmitting}
                    onclick={() => chat.recoverHistoryFork(operation)}>{$t('chat.recoverEdit')}</Button
                  >
                {/if}
              </div>
            </Alert.Description>
          </Alert.Root>
        {/each}

        {#if chat.branchActionError}
          <Alert.Root variant="destructive" role="alert" class="mx-4 mb-4">
            <WarningCircleIcon class="size-4" />
            <Alert.Title>{$t('chat.branchFailed')}</Alert.Title>
            <Alert.Description>{chat.branchActionError}</Alert.Description>
          </Alert.Root>
        {/if}
        <div
          aria-hidden="true"
          class="absolute bottom-0 h-px w-px"
          data-chat-bottom-sentinel
          bind:this={bottomSentinel}
        ></div>
        <ScrollToBottomButton
          bind:this={scrollDownButton}
          sentinel={bottomSentinel}
          onclick={chat.scrollChatToBottom}
          class="chat-scroll-button fixed bottom-[calc(var(--chat-composer-height)+0.5rem)] left-0 right-0 lg:right-[var(--file-preview-width,0px)] z-40 px-4 transition-[left] duration-200 ease-linear md:left-[var(--sidebar-width)] md:px-8 group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]"
        />

        {@render dock?.()}
      {/if}
    </div>
  </div>
</section>
