<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { Bot, Check, Copy, Pencil, RefreshCw } from '@lucide/svelte'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import { copyText } from '$lib/copyText'
  import AgentBottomStatus from './AgentBottomStatus.svelte'
  import AgentStatus from './AgentStatus.svelte'
  import ThoughtSummary from './ThoughtSummary.svelte'
  import type { SessionChatMessage } from '../../session-chat/sessionChat'

  interface Props {
    messages: SessionChatMessage[]
    sessionState?: string | null
    loading?: boolean
    interruptEnabled?: boolean
    interruptBusy?: boolean
    hasMoreHistory?: boolean
    historyLoading?: boolean
    historyObserverEnabled?: boolean
    branchActionInputs?: Record<string, string>
    branchActionBusy?: boolean
    onBranchEdit?: (message: SessionChatMessage, replacementInput: string) => boolean | Promise<boolean>
    onBranchResend?: (message: SessionChatMessage) => void | Promise<void>
    onInterrupt?: () => void
    onLoadMoreHistory?: () => void | Promise<void>
  }

  let {
    messages,
    sessionState = null,
    loading = false,
    interruptEnabled: _interruptEnabled = false,
    interruptBusy: _interruptBusy = false,
    hasMoreHistory = false,
    historyLoading = false,
    historyObserverEnabled = false,
    branchActionInputs = {},
    branchActionBusy = false,
    onBranchEdit,
    onBranchResend,
    onInterrupt: _onInterrupt,
    onLoadMoreHistory,
  }: Props = $props()
  let scrollContainer = $state<HTMLDivElement | null>(null)
  let copiedMessageId = $state<string | null>(null)
  let editingMessageId = $state<string | null>(null)
  let editedInput = $state('')
  let copiedMessageResetTimer: ReturnType<typeof setTimeout> | null = null
  const displayMessages = $derived(messages)
  const displayItems = $derived(conversationDisplayItems(displayMessages, sessionState))
  const displayGroups = $derived(conversationDisplayGroups(displayItems))
  const latestAssistantGroupId = $derived([...displayGroups].reverse().find((group) => group.kind === 'assistant_group')?.id ?? null)
  const activeLoadingMessageId = $derived(lastEmptyPendingAssistantMessageId(displayMessages))
  const branchActionMessageIdSet = $derived(new Set(Object.keys(branchActionInputs)))
  let topHistoryLoadInFlight = false
  let topHistorySentinelVisible = $state(false)
  let topHistoryPullDistance = $state(0)
  let topHistoryTouchStartY: number | null = null
  const TOP_HISTORY_PULL_THRESHOLD_PX = 96

  onDestroy(() => {
    if (copiedMessageResetTimer) clearTimeout(copiedMessageResetTimer)
  })

  function lastEmptyPendingAssistantMessageId(chatMessages: SessionChatMessage[]): string | null {
    const message = chatMessages.at(-1)
    if (message?.role === 'assistant' && message.status === 'pending' && !message.content.trim()) return message.id
    return null
  }

  async function copyAssistantReply(message: SessionChatMessage): Promise<void> {
    const copied = await copyText(message.content)
    if (!copied) return
    copiedMessageId = message.id
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
    const submitted = await onBranchEdit?.(message, editedInput.trim())
    if (submitted) cancelEditing()
  }

  function handleEditKeydown(event: KeyboardEvent, message: SessionChatMessage): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      cancelEditing()
      return
    }
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault()
      void submitEdit(message)
    }
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
    | { kind: 'message'; id: string; message: SessionChatMessage; showAgentStatus: boolean }
    | { kind: 'agent_status'; id: string }
    | { kind: 'agent_bottom_status'; id: string }

  type ConversationDisplayGroup =
    | { kind: 'user_message'; id: string; item: Extract<ConversationDisplayItem, { kind: 'message' }> }
    | { kind: 'assistant_group'; id: string; items: ConversationDisplayItem[] }

  function conversationDisplayItems(chatMessages: SessionChatMessage[], state: string | null): ConversationDisplayItem[] {
    const showBottomStatus = state === 'starting' || state === 'exited' || state === 'interrupted'
    const showStatus = Boolean(state && state !== 'idle' && !showBottomStatus)
    const latestAssistantId = chatMessages.at(-1)?.role === 'assistant' ? chatMessages.at(-1)?.id : null
    const items: ConversationDisplayItem[] = chatMessages.map((message) => ({
      kind: 'message',
      id: message.id,
      message,
      showAgentStatus: showStatus && message.id === latestAssistantId,
    }))
    if (showBottomStatus) return [...items, { kind: 'agent_bottom_status', id: `agent-bottom-status:${state}` }]
    if (!showStatus || latestAssistantId) return items
    return [...items, { kind: 'agent_status', id: `agent-status:${state}` }]
  }

  function conversationDisplayGroups(items: ConversationDisplayItem[]): ConversationDisplayGroup[] {
    const groups: ConversationDisplayGroup[] = []
    let assistantGroup: Extract<ConversationDisplayGroup, { kind: 'assistant_group' }> | null = null

    function flushAssistantGroup(): void {
      if (!assistantGroup) return
      groups.push(assistantGroup)
      assistantGroup = null
    }

    for (const item of items) {
      if (item.kind === 'message' && item.message.role === 'user') {
        flushAssistantGroup()
        groups.push({ kind: 'user_message', id: `user:${item.id}`, item })
        continue
      }

      assistantGroup ??= { kind: 'assistant_group', id: `assistant-group:${item.id}`, items: [] }
      assistantGroup.items.push(item)
    }

    flushAssistantGroup()
    return groups
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

  function preserveDocumentScrollAnchor(previousScrollHeight: number, previousScrollY: number): void {
    const scrollHeightDelta = document.documentElement.scrollHeight - previousScrollHeight
    if (scrollHeightDelta <= 0) return
    window.scrollTo({ top: previousScrollY + scrollHeightDelta })
  }

  async function loadMoreHistoryFromTop(): Promise<void> {
    topHistoryLoadInFlight = true
    const previousScrollHeight = document.documentElement.scrollHeight
    const previousScrollY = window.scrollY
    try {
      await onLoadMoreHistory?.()
      await tick()
      await nextAnimationFrame()
      preserveDocumentScrollAnchor(previousScrollHeight, previousScrollY)
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

{#snippet conversationItem(displayItem: ConversationDisplayItem)}
  {#if displayItem.kind === 'agent_status'}
    <Message.Root from="assistant" data-chat-agent-status>
      <Message.Content>
        <AgentStatus state={sessionState} interruptEnabled={_interruptEnabled} interruptBusy={_interruptBusy} onInterrupt={_onInterrupt} />
      </Message.Content>
    </Message.Root>
  {:else if displayItem.kind === 'agent_bottom_status'}
    <AgentBottomStatus state={sessionState} />
  {:else}
    {@const chatMessage = displayItem.message}
    <Message.Root from={chatMessage.role} data-chat-message-id={chatMessage.id}>
      <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
        {#if displayItem.showAgentStatus}
          <AgentStatus state={sessionState} interruptEnabled={_interruptEnabled} interruptBusy={_interruptBusy} onInterrupt={_onInterrupt} />
        {/if}
        {#if chatMessage.role === 'assistant' && chatMessage.thoughtSteps?.length}
          <ThoughtSummary class="mb-3" steps={chatMessage.thoughtSteps} active={(sessionState ? sessionState === 'busy' : true) && chatMessage.id === activeLoadingMessageId} />
        {/if}
        {#if chatMessage.content.trim()}
          {#if chatMessage.role === 'user' && editingMessageId === chatMessage.id}
            <div class="w-full min-w-[min(32rem,75vw)] space-y-3">
              <Textarea
                aria-label="Edit historical message"
                bind:value={editedInput}
                rows={4}
                disabled={branchActionBusy}
                onkeydown={(event) => handleEditKeydown(event, chatMessage)}
              />
              <p class="text-xs text-muted-foreground">
                Editing restores conversational context, but does not rewind workspace files or external side effects.
              </p>
              <div class="flex justify-end gap-2">
                <Button type="button" variant="ghost" size="sm" disabled={branchActionBusy} onclick={cancelEditing}>
                  Cancel editing
                </Button>
                <Button
                  type="button"
                  size="sm"
                  disabled={branchActionBusy || !editedInput.trim()}
                  onclick={() => void submitEdit(chatMessage)}
                >
                  Send edit
                </Button>
              </div>
            </div>
          {:else}
            <Message.Response content={chatMessage.content} markdown={chatMessage.role === 'assistant'} />
          {/if}
          {#if chatMessage.role === 'assistant'}
            {@const isCopied = copiedMessageId === chatMessage.id}
            <div class="mt-2 flex justify-start">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                aria-label={isCopied ? 'Assistant reply copied' : 'Copy assistant reply'}
                title={isCopied ? 'Copied' : 'Copy assistant reply'}
                onclick={() => copyAssistantReply(chatMessage)}
              >
                {#if isCopied}
                  <Check class="size-3.5" /> Copied
                {:else}
                  <Copy class="size-3.5" /> Copy
                {/if}
              </Button>
            </div>
          {:else if branchActionMessageIdSet.has(chatMessage.id) && editingMessageId !== chatMessage.id}
            <div class="mt-2 flex justify-end gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={branchActionBusy}
                aria-label={`Edit message: ${chatMessage.content}`}
                onclick={() => beginEditing(chatMessage)}
              >
                <Pencil class="size-3.5" /> Edit
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={branchActionBusy}
                aria-label={`Resend message: ${chatMessage.content}`}
                onclick={() => void onBranchResend?.(chatMessage)}
              >
                <RefreshCw class="size-3.5" /> Resend
              </Button>
            </div>
          {/if}
        {/if}
      </Message.Content>
    </Message.Root>

  {/if}
{/snippet}

<Conversation.Root class="h-auto min-h-0 min-w-0 flex-1 overflow-visible">
  {#if loading}
    <Conversation.EmptyState title="Loading conversation…" description="Fetching the latest session transcript." />
  {:else if !displayMessages.length}
    <Empty.Root class="h-full">
      <Empty.Header>
        <Empty.Media><Bot class="size-6" /></Empty.Media>
        <Empty.Title>No messages yet</Empty.Title>
        <Empty.Description>This session has no turn history yet.</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Conversation.Content bind:ref={scrollContainer} data-chat-conversation-content class="min-w-0 overflow-visible px-0 py-4 sm:p-4">
      {#if historyObserverEnabled && hasMoreHistory}
        <div aria-hidden="true" class="h-px w-px" data-chat-history-top-sentinel use:observeTopHistorySentinel></div>
      {/if}
      {#if hasMoreHistory && historyLoading}
        <div class="pb-2 text-center text-xs text-muted-foreground" role="status" aria-live="polite">Loading earlier messages…</div>
      {:else if historyObserverEnabled && hasMoreHistory && topHistorySentinelVisible}
        <div class="pb-2 text-center text-xs text-muted-foreground" role="status" aria-live="polite" data-chat-history-pull-hint>
          {topHistoryPullDistance >= TOP_HISTORY_PULL_THRESHOLD_PX ? 'Release to load earlier messages' : 'Keep scrolling up to load earlier messages'}
        </div>
      {/if}
      {#each displayGroups as displayGroup (displayGroup.id)}
        {#if displayGroup.kind === 'user_message'}
          {@render conversationItem(displayGroup.item)}
        {:else}
          <div class={displayGroup.id === latestAssistantGroupId ? 'chat-turn-tail-space' : ''} data-chat-assistant-group>
            {#each displayGroup.items as displayItem (displayItem.id)}
              {@render conversationItem(displayItem)}
            {/each}
          </div>
        {/if}
      {/each}
    </Conversation.Content>
  {/if}
</Conversation.Root>

<style>
  :global(.chat-turn-tail-space) {
    /*
      Keep the latest turn high enough to pin fresh user input near the top,
      while accounting for the sticky header, a one-line user bubble, inline
      agent status, and a collapsed thought summary inside the live turn.
    */
    min-height: calc(100dvh - 31rem);
  }
</style>
