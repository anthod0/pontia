<script lang="ts">
  import { t } from '$lib/i18n';
  import SentSecrets from '../../../components/chat/SentSecrets.svelte'
  import UploadedFiles from '../../../components/chat/files/UploadedFiles.svelte'
  import { parseUploadMessage } from '$lib/uploads/message'
  import { parseQuestionReplies, questionRepliesText } from '$lib/session-chat/questionReply'
  import { onDestroy, tick, type Snippet } from 'svelte'
  import RobotIcon from 'phosphor-svelte/lib/RobotIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { copyText } from '$lib/copyText'
  import SessionConversationSkeleton from './SessionConversationSkeleton.svelte'
  import AgentBottomStatus from './AgentBottomStatus.svelte'
  import AgentStatus from './AgentStatus.svelte'
  import ThoughtSummary from './ThoughtSummary.svelte'
  import TurnDuration from './TurnDuration.svelte'
  import CompactionCheckpoint from './CompactionCheckpoint.svelte'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import QuestionReplyCard from './QuestionReplyCard.svelte'
  import SharedFiles from '../../../components/chat/files/SharedFiles.svelte'
  import { splitAttachments } from '$lib/attachmentBlocks'
  import type { TurnView, TimelineItem } from '../../../api/types'
  import type { SessionChatMessage } from '../../session-chat/sessionChat'

  interface Props {
    stepsCard?: Snippet<[NonNullable<SessionChatMessage['thoughtSteps']>, boolean, TurnView | undefined]>
    messages: SessionChatMessage[]
    timelineItems?: TimelineItem[]
    compactionBusy?: boolean
    turns?: TurnView[]
    sessionId?: string
    /** Session workspace directory; relative local file links in messages resolve against it. */
    workspace?: string | null
    sessionState?: string | null
    activeTurnId?: string | null
    loading?: boolean
    hasMoreHistory?: boolean
    historyLoading?: boolean
    historyObserverEnabled?: boolean
    branchActionInputs?: Record<string, string>
    branchActionBusy?: boolean
    onBranchEdit?: (message: SessionChatMessage, replacementInput: string) => boolean | Promise<boolean>
    onLoadMoreHistory?: () => void | Promise<void>
  }

  let {
    stepsCard,
    messages,
    timelineItems = [],
    compactionBusy = false,
    turns = [],
    sessionId = '',
    workspace = null,
    sessionState = null,
    activeTurnId = null,
    loading = false,
    hasMoreHistory = false,
    historyLoading = false,
    historyObserverEnabled = false,
    branchActionInputs = {},
    branchActionBusy = false,
    onBranchEdit,
    onLoadMoreHistory,
  }: Props = $props()
  let scrollContainer = $state<HTMLDivElement | null>(null)
  let copiedMessageId = $state<string | null>(null)
  let editingMessageId = $state<string | null>(null)
  let editedInput = $state('')
  let copiedMessageResetTimer: ReturnType<typeof setTimeout> | null = null
  const displayMessages = $derived(messages)
  const displayItems = $derived(conversationDisplayItems(displayMessages, timelineItems, compactionBusy))
  const displayTurns = $derived(conversationDisplayTurns(displayItems))
  const turnsById = $derived(new Map(turns.map((turn) => [turn.turn_id, turn])))
  const busy = $derived(sessionState === 'busy')
  const showBottomStatus = $derived(sessionState === 'starting' || sessionState === 'exited' || sessionState === 'interrupted')
  // The Turn whose footer shows live progress: the active Turn, else the latest one.
  const progressTurnId = $derived(
    displayTurns.find((turn) => turn.turnId !== null && turn.turnId === activeTurnId)?.id ?? displayTurns.at(-1)?.id ?? null,
  )
  const branchActionMessageIdSet = $derived(new Set(Object.keys(branchActionInputs)))
  let topHistoryLoadInFlight = false
  let topHistorySentinelVisible = $state(false)
  let topHistoryPullDistance = $state(0)
  let topHistoryTouchStartY: number | null = null
  const TOP_HISTORY_PULL_THRESHOLD_PX = 96

  onDestroy(() => {
    if (copiedMessageResetTimer) clearTimeout(copiedMessageResetTimer)
  })

  async function copyMessage(id: string, content: string): Promise<void> {
    const copied = await copyText(content)
    if (!copied) return
    copiedMessageId = id
    if (copiedMessageResetTimer) clearTimeout(copiedMessageResetTimer)
    copiedMessageResetTimer = setTimeout(() => {
      copiedMessageId = null
      copiedMessageResetTimer = null
    }, 1600)
  }

  function beginEditing(message: SessionChatMessage): void {
    editingMessageId = message.id
    editedInput = branchActionInputs[message.id] ?? message.content
  }

  function cancelEditing(): void {
    editingMessageId = null
    editedInput = ''
  }

  async function submitEdit(message: SessionChatMessage): Promise<void> {
    if (!editedInput.trim() || branchActionBusy) return
    const submitted = await onBranchEdit?.(message, editedInput)
    if (submitted) cancelEditing()
  }

  function handleEditKeydown(event: KeyboardEvent, message: SessionChatMessage): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      cancelEditing()
      return
    }

    const isPlainEnter = event.key === 'Enter'
      && !event.shiftKey
      && !event.ctrlKey
      && !event.metaKey
      && !event.altKey
    if (!isPlainEnter || event.isComposing) return

    event.preventDefault()
    void submitEdit(message)
  }

  function observeTopHistorySentinel(node: HTMLElement): { destroy: () => void } {
    if (typeof IntersectionObserver === 'undefined') return { destroy: () => undefined }
    const observer = new IntersectionObserver((entries) => {
      topHistorySentinelVisible = entries.some((entry) => entry.isIntersecting)
      if (!topHistorySentinelVisible) resetTopHistoryPull()
    })
    observer.observe(node)
    return {
      destroy: () => {
        observer.disconnect()
        topHistorySentinelVisible = false
        resetTopHistoryPull()
      },
    }
  }

  type ConversationDisplayItem =
    | { kind: 'message'; id: string; message: SessionChatMessage }
    | { kind: 'compaction'; id: string; checkpoint: TimelineItem | null }

  function conversationDisplayItems(chatMessages: SessionChatMessage[], nativeItems: TimelineItem[], compacting: boolean): ConversationDisplayItem[] {
    const items: ConversationDisplayItem[] = chatMessages.map((message) => ({ kind: 'message', id: message.id, message }))
    const positions = new Map(nativeItems.map((item, index) => [item.item_id, index]))
    for (const [index, checkpoint] of nativeItems.entries()) {
      if (checkpoint.kind !== 'context_compaction') continue
      const nextIndex = items.findIndex(item => {
        if (item.kind !== 'message') return false
        const message = item.message
        const position = message.thoughtSteps?.map(step => positions.get(step.id)).find(position => position !== undefined)
          ?? positions.get(message.id)
        if (position === undefined && message.role === 'assistant' && message.status === 'pending' && message.turnId === checkpoint.turn_id) return true
        return position !== undefined ? position > index : Boolean(checkpoint.occurred_at && message.createdAt > checkpoint.occurred_at)
      })
      items.splice(nextIndex < 0 ? items.length : nextIndex, 0, { kind: 'compaction', id: `compaction:${checkpoint.item_id}`, checkpoint })
    }
    if (compacting) items.push({ kind: 'compaction', id: 'compaction:pending', checkpoint: null })
    return items
  }

  interface ConversationDisplayTurn {
    id: string
    turnId: string | null
    items: ConversationDisplayItem[]
    reply: string
    repliedAt: string
  }

  function conversationDisplayTurns(items: ConversationDisplayItem[]): ConversationDisplayTurn[] {
    const turns: ConversationDisplayTurn[] = []
    for (const item of items) {
      let turn = turns.at(-1)
      const itemTurnId = item.kind === 'message' ? item.message.turnId ?? null : item.checkpoint?.turn_id ?? turn?.turnId ?? null
      if (!turn || itemTurnId !== turn.turnId) {
        turn = { id: item.id, turnId: itemTurnId, items: [], reply: '', repliedAt: '' }
        turns.push(turn)
      }
      turn.items.push(item)
      if (item.kind === 'message' && item.message.role === 'assistant' && item.message.content.trim()) {
        turn.reply = turn.reply ? `${turn.reply}\n\n${item.message.content}` : item.message.content
        turn.repliedAt = item.message.createdAt || turn.repliedAt
      }
    }
    return turns
  }

  function lastMessageId(turn: ConversationDisplayTurn): string | null {
    return turn.items.findLast((item) => item.kind === 'message')?.id ?? null
  }

  function topHistoryPullReady(): boolean {
    return historyObserverEnabled && hasMoreHistory && topHistorySentinelVisible && !historyLoading && !topHistoryLoadInFlight && Boolean(onLoadMoreHistory)
  }

  function resetTopHistoryPull(): void {
    topHistoryPullDistance = 0
    topHistoryTouchStartY = null
  }

  function maybeLoadMoreHistoryFromPull(): void {
    if (!topHistoryPullReady()) return
    if (topHistoryPullDistance < TOP_HISTORY_PULL_THRESHOLD_PX) return
    resetTopHistoryPull()
    void loadMoreHistoryFromTop()
  }

  function handleHistoryWheel(event: WheelEvent): void {
    if (!topHistoryPullReady()) {
      if (!topHistorySentinelVisible || event.deltaY > 0) resetTopHistoryPull()
      return
    }

    if (event.deltaY < 0) {
      topHistoryPullDistance += Math.abs(event.deltaY)
      maybeLoadMoreHistoryFromPull()
      return
    }

    resetTopHistoryPull()
  }

  function handleHistoryTouchStart(event: TouchEvent): void {
    if (!topHistoryPullReady()) return
    topHistoryTouchStartY = event.touches[0]?.clientY ?? null
    topHistoryPullDistance = 0
  }

  function handleHistoryTouchMove(event: TouchEvent): void {
    if (!topHistoryPullReady() || topHistoryTouchStartY === null) return
    const currentY = event.touches[0]?.clientY
    if (currentY === undefined) return
    topHistoryPullDistance = Math.max(0, currentY - topHistoryTouchStartY)
  }

  function handleHistoryTouchEnd(): void {
    maybeLoadMoreHistoryFromPull()
    if (!topHistoryLoadInFlight) resetTopHistoryPull()
  }

  function nextAnimationFrame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()))
  }

  async function loadMoreHistoryFromTop(): Promise<void> {
    topHistoryLoadInFlight = true
    const mobileBody = scrollContainer?.closest<HTMLElement>('.mobile-body')
    const scroller = mobileBody ?? document.documentElement
    const previousScrollHeight = scroller.scrollHeight
    const previousScrollY = mobileBody?.scrollTop ?? window.scrollY
    try {
      await onLoadMoreHistory?.()
      await tick()
      await nextAnimationFrame()
      const delta = scroller.scrollHeight - previousScrollHeight
      if (delta > 0) (mobileBody ?? window).scrollTo({top: previousScrollY + delta})
    } finally {
      topHistoryLoadInFlight = false
    }
  }

</script>

<svelte:window
  onwheel={handleHistoryWheel}
  ontouchstart={handleHistoryTouchStart}
  ontouchmove={handleHistoryTouchMove}
  ontouchend={handleHistoryTouchEnd}
  ontouchcancel={resetTopHistoryPull}
/>

{#snippet copyButton(id: string, content: string, label: string, copiedLabel: string)}
  {@const isCopied = copiedMessageId === id}
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label={isCopied ? copiedLabel : label}
    title={isCopied ? $t('common.copied') : label}
    onclick={() => copyMessage(id, content)}
  >
    {#if isCopied}
      <CheckIcon class="size-3.5" />
    {:else}
      <CopyIcon class="size-3.5" />
    {/if}
  </Button>
{/snippet}

{#snippet conversationItem(displayItem: ConversationDisplayItem, liveSegment: boolean)}
  {#if displayItem.kind === 'compaction'}
    <CompactionCheckpoint item={displayItem.checkpoint} busy={displayItem.checkpoint === null} />
  {:else}
    {@const chatMessage = displayItem.message}
    {@const uploadParts = chatMessage.role === 'user' ? parseUploadMessage(chatMessage.content) : null}
    {@const questionReplies = uploadParts ? parseQuestionReplies(uploadParts.text) : null}
    {@const copyContent = questionReplies ? questionRepliesText(questionReplies) : chatMessage.content}
    <Message.Root
      from={chatMessage.role}
      class={`${chatMessage.role === 'user' ? `relative ${editingMessageId === chatMessage.id ? 'w-full' : ''} ${chatMessage.status === 'pending' ? 'opacity-60' : ''}` : ''} scroll-mt-16`}
      data-chat-message-id={chatMessage.id}
      data-chat-turn-id={chatMessage.turnId}
      data-chat-role={chatMessage.role}
    >
      {#if uploadParts?.secrets.length && editingMessageId !== chatMessage.id}
        <SentSecrets secrets={uploadParts.secrets} />
      {/if}
      {#if uploadParts?.files.length && editingMessageId !== chatMessage.id}
        <UploadedFiles files={uploadParts.files} />
      {/if}
      {#if chatMessage.role === 'assistant'}
        {#if chatMessage.thoughtSteps?.length}
          {@const active = !!(liveSegment && busy && !chatMessage.content.trim())}
          {#if stepsCard}
            {@render stepsCard(chatMessage.thoughtSteps, active, turnsById.get(chatMessage.turnId))}
          {:else}
            <ThoughtSummary steps={chatMessage.thoughtSteps} {active} />
          {/if}
        {/if}
        {#each splitAttachments(chatMessage.content, chatMessage.status === 'pending') as part, index (index)}
          {#if part.kind === 'text'}
            <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
              <Message.Response
                content={part.text}
                {sessionId}
                linkBase={workspace ? `${workspace.replace(/\/+$/, '')}/` : null}
                markdown
                streaming={chatMessage.status === 'pending'}
                streamId={`${chatMessage.id}:${index}`}
              />
            </Message.Content>
          {:else if sessionId}
            <div class="w-full max-w-md" data-chat-message-files><SharedFiles paths={part.paths} {sessionId} /></div>
          {/if}
        {/each}
      {:else if !uploadParts || uploadParts.text.trim() || editingMessageId === chatMessage.id}
        <Message.Content class={`${chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''} ${editingMessageId === chatMessage.id ? 'w-full' : ''}`}>
          {#if editingMessageId === chatMessage.id}
            <div class="w-full space-y-3">
              <textarea
                aria-label={$t('chat.editHistoricalMessage')}
                bind:value={editedInput}
                class="field-sizing-content block w-full resize-none border-0 bg-transparent p-0 text-base leading-relaxed outline-none disabled:cursor-not-allowed disabled:opacity-50"
                disabled={branchActionBusy}
                onkeydown={(event) => handleEditKeydown(event, chatMessage)}
              ></textarea>
              <div class="flex justify-end gap-2">
                <Button type="button" variant="ghost" size="sm" disabled={branchActionBusy} onclick={cancelEditing}>
                  {$t('common.cancel')}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  disabled={branchActionBusy || !editedInput.trim()}
                  onclick={() => void submitEdit(chatMessage)}
                >
                  {$t('common.save')}
                </Button>
              </div>
            </div>
          {:else if questionReplies}
            <QuestionReplyCard replies={questionReplies} />
          {:else}
            <Message.Response content={uploadParts?.text ?? ''} markdown={false} />
          {/if}
        </Message.Content>
      {/if}
      {#if chatMessage.role === 'user' && editingMessageId !== chatMessage.id}
        <div
          class="flex items-center justify-end gap-0.5 text-muted-foreground"
          data-user-message-actions
        >
          {#if chatMessage.createdAt}<MessageTime value={chatMessage.createdAt} />{/if}
          {#if branchActionMessageIdSet.has(chatMessage.id)}
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              disabled={branchActionBusy}
              aria-label={$t('chat.editMessageNamed', { message: chatMessage.content })}
              title={$t('chat.editMessage')}
              onclick={() => beginEditing(chatMessage)}
            >
              <PencilSimpleIcon class="size-3.5" />
            </Button>
          {/if}
          {@render copyButton(chatMessage.id, copyContent, $t('chat.copyUserMessage', { message: copyContent }), $t('chat.userMessageCopied', { message: copyContent }))}
        </div>
      {/if}
    </Message.Root>
  {/if}
{/snippet}

{#snippet turnFooter(displayTurn: ConversationDisplayTurn, showProgress: boolean)}
  {@const turn = displayTurn.turnId ? turnsById.get(displayTurn.turnId) : undefined}
  {@const running = showProgress && busy}
  {@const showDuration = running || turn?.state === 'completed'}
  {@const status = showProgress && !busy && !showBottomStatus && sessionState !== 'idle' ? sessionState : null}
  {#if showDuration || displayTurn.reply || status}
    <div class="not-prose flex min-w-0 flex-wrap items-center gap-x-1 text-sm text-muted-foreground" data-chat-turn-footer>
      {#if status}
        <AgentStatus state={status} class="pt-0" />
      {/if}
      {#if showDuration}
        <span class="px-1 text-xs"><TurnDuration {turn} active={running} /></span>
      {/if}
      {#if displayTurn.reply && !running}
        {#if displayTurn.repliedAt}<MessageTime value={displayTurn.repliedAt} />{/if}
        {@render copyButton(`reply:${displayTurn.id}`, displayTurn.reply, $t('chat.copyReply'), $t('chat.replyCopied'))}
      {/if}
    </div>
  {/if}
{/snippet}

<Conversation.Root class="h-auto min-h-0 min-w-0 flex-1 overflow-visible">
  {#if loading}
    <SessionConversationSkeleton class="min-h-80" />
  {:else if !displayMessages.length && !compactionBusy && !timelineItems.some(item => item.kind === 'context_compaction')}
    <Empty.Root class="h-full">
      <Empty.Header>
        <Empty.Media><RobotIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('chat.noMessages')}</Empty.Title>
        <Empty.Description>{$t('chat.noTurns')}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Conversation.Content bind:ref={scrollContainer} data-chat-conversation-content class="min-w-0 gap-6 overflow-visible px-0 pb-0 pt-4">
      {#if historyObserverEnabled && hasMoreHistory}
        <div aria-hidden="true" class="h-px w-px" data-chat-history-top-sentinel use:observeTopHistorySentinel></div>
      {/if}
      {#if hasMoreHistory && historyLoading}
        <div class="pb-2 text-center text-xs text-muted-foreground" role="status" aria-live="polite">{$t('chat.loadingEarlier')}</div>
      {:else if historyObserverEnabled && hasMoreHistory && topHistorySentinelVisible}
        <div class="pb-2 text-center text-xs text-muted-foreground" role="status" aria-live="polite" data-chat-history-pull-hint>
          {topHistoryPullDistance >= TOP_HISTORY_PULL_THRESHOLD_PX ? $t('chat.releaseToLoad') : $t('chat.scrollToLoad')}
        </div>
      {/if}
      {#each displayTurns as displayTurn, index (displayTurn.id)}
        {@const showProgress = displayTurn.id === progressTurnId}
        {@const liveMessageId = showProgress ? lastMessageId(displayTurn) : null}
        <div
          data-chat-turn={displayTurn.turnId ? '' : undefined}
          data-chat-turn-id={displayTurn.turnId}
          class="flex min-w-0 flex-col gap-4"
          class:latest-turn={index === displayTurns.length - 1 && displayTurn.turnId !== null}
        >
          {#each displayTurn.items as displayItem (displayItem.id)}
            {@render conversationItem(displayItem, displayItem.id === liveMessageId)}
          {/each}
          {@render turnFooter(displayTurn, showProgress)}
          {#if showBottomStatus && index === displayTurns.length - 1}
            <AgentBottomStatus state={sessionState} class="py-0" />
          {/if}
        </div>
      {/each}
    </Conversation.Content>
  {/if}
</Conversation.Root>


<style>
  .latest-turn {
    min-height: max(0px, calc(100dvh - var(--chat-top-offset, 4rem) - var(--chat-bottom-padding, 0px)));
  }
</style>
