<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import { Bot, Check, Copy, GitBranch } from '@lucide/svelte'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { copyText } from '$lib/copyText'
  import { chatAutoScrollKey, scrollDocumentToBottom } from '../../session-chat/autoScroll'
  import DraftDagFlow from '../../../components/dag/DraftDagFlow.svelte'
  import AgentExitStatus from './AgentExitStatus.svelte'
  import AgentStatus from './AgentStatus.svelte'
  import ThoughtSummary from './ThoughtSummary.svelte'
  import type { DagProposalView, JsonObject } from '../../../api/types'
  import type { SessionChatMessage } from '../../session-chat/sessionChat'

  interface Props {
    messages: SessionChatMessage[]
    sessionState?: string | null
    loading?: boolean
    plannerTaskId?: string | null
    draftPlannerProposal?: DagProposalView | null
    draftPlannerProposalLoading?: boolean
    interruptEnabled?: boolean
    interruptBusy?: boolean
    hasMoreHistory?: boolean
    historyLoading?: boolean
    autoScrollKey?: string | null
    onInterrupt?: () => void
    onLoadMoreHistory?: () => void | Promise<void>
  }

  let {
    messages,
    sessionState = null,
    loading = false,
    plannerTaskId = null,
    draftPlannerProposal = null,
    draftPlannerProposalLoading = false,
    interruptEnabled: _interruptEnabled = false,
    interruptBusy: _interruptBusy = false,
    hasMoreHistory = false,
    historyLoading = false,
    autoScrollKey = null,
    onInterrupt: _onInterrupt,
    onLoadMoreHistory,
  }: Props = $props()
  let scrollContainer = $state<HTMLDivElement | null>(null)
  let draftDagSheetOpen = $state(false)
  let copiedMessageId = $state<string | null>(null)
  let copiedMessageResetTimer: ReturnType<typeof setTimeout> | null = null
  const displayMessages = $derived(messages)
  const displayItems = $derived(conversationDisplayItems(displayMessages, sessionState))
  const scrollKey = $derived(autoScrollKey ?? chatAutoScrollKey(displayMessages))
  const plannerDraftAnchorId = $derived(lastAssistantMessageId(displayMessages))
  const activeLoadingMessageId = $derived(lastEmptyPendingAssistantMessageId(displayMessages))
  const TOP_HISTORY_LOAD_THRESHOLD_PX = 80
  const BOTTOM_AUTO_SCROLL_THRESHOLD_PX = 160
  let topHistoryLoadInFlight = false
  let previousScrollKey: string | null = null
  let shouldAutoScrollAfterUpdate = false

  onMount(() => {
    window.addEventListener('scroll', handleWindowScroll, { passive: true })
  })

  onDestroy(() => {
    window.removeEventListener('scroll', handleWindowScroll)
    if (copiedMessageResetTimer) clearTimeout(copiedMessageResetTimer)
  })

  $effect.pre(() => {
    scrollKey
    shouldAutoScrollAfterUpdate = previousScrollKey === null || isDocumentNearBottom()
  })

  $effect(() => {
    const nextScrollKey = scrollKey
    if (previousScrollKey === null) {
      previousScrollKey = nextScrollKey
      return
    }
    if (previousScrollKey === nextScrollKey) return
    previousScrollKey = nextScrollKey
    if (shouldAutoScrollAfterUpdate) void tick().then(scrollDocumentToBottom)
  })

  function isDocumentNearBottom(): boolean {
    const distanceFromBottom = document.documentElement.scrollHeight - (window.scrollY + window.innerHeight)
    return distanceFromBottom <= BOTTOM_AUTO_SCROLL_THRESHOLD_PX
  }

  function lastAssistantMessageId(chatMessages: SessionChatMessage[]): string | null {
    for (let index = chatMessages.length - 1; index >= 0; index -= 1) {
      if (chatMessages[index]?.role === 'assistant') return chatMessages[index].id
    }
    return null
  }

  function lastEmptyPendingAssistantMessageId(chatMessages: SessionChatMessage[]): string | null {
    const message = chatMessages.at(-1)
    if (message?.role === 'assistant' && message.status === 'pending' && !message.content.trim()) return message.id
    return null
  }

  function proposalWorkItems(proposal: DagProposalView | null): JsonObject[] {
    const workItems = proposal?.proposal_json.work_items
    return Array.isArray(workItems) ? workItems.filter(isJsonObject) : []
  }

  function proposalEdges(proposal: DagProposalView | null): JsonObject[] {
    const edges = proposal?.proposal_json.edges
    return Array.isArray(edges) ? edges.filter(isJsonObject) : []
  }

  function isJsonObject(value: unknown): value is JsonObject {
    return Boolean(value && typeof value === 'object' && !Array.isArray(value))
  }

  function openDraftDagSheet(): void {
    draftDagSheetOpen = true
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

  $effect(() => {
    displayMessages.length
    hasMoreHistory
    historyLoading
    void tick().then(maybeLoadMoreHistoryFromTop)
  })

  function handleWindowScroll(): void {
    void maybeLoadMoreHistoryFromTop()
  }

  function maybeLoadMoreHistoryFromTop(): void {
    if (window.scrollY > TOP_HISTORY_LOAD_THRESHOLD_PX) return
    if (!hasMoreHistory || historyLoading || topHistoryLoadInFlight || !onLoadMoreHistory) return
    void loadMoreHistoryFromTop()
  }

  type ConversationDisplayItem =
    | { kind: 'message'; id: string; message: SessionChatMessage; showAgentStatus: boolean }
    | { kind: 'agent_status'; id: string }
    | { kind: 'agent_exit_status'; id: string }

  function conversationDisplayItems(chatMessages: SessionChatMessage[], state: string | null): ConversationDisplayItem[] {
    const showExitStatus = state === 'exited'
    const showStatus = Boolean(state && state !== 'idle' && !showExitStatus)
    const latestAssistantId = chatMessages.at(-1)?.role === 'assistant' ? chatMessages.at(-1)?.id : null
    const items: ConversationDisplayItem[] = chatMessages.map((message) => ({
      kind: 'message',
      id: message.id,
      message,
      showAgentStatus: showStatus && message.id === latestAssistantId,
    }))
    if (showExitStatus) return [...items, { kind: 'agent_exit_status', id: 'agent-exit-status' }]
    if (!showStatus || latestAssistantId) return items
    return [...items, { kind: 'agent_status', id: `agent-status:${state}` }]
  }

  interface ScrollAnchor {
    messageId: string
    top: number
  }

  function messageElements(): HTMLElement[] {
    return Array.from(document.querySelectorAll<HTMLElement>('[data-chat-message-id]'))
  }

  function captureScrollAnchor(): ScrollAnchor | null {
    const anchor = messageElements().find((element) => element.getBoundingClientRect().bottom > 0)
    if (!anchor?.dataset.chatMessageId) return null
    return { messageId: anchor.dataset.chatMessageId, top: anchor.getBoundingClientRect().top }
  }

  function restoreScrollAnchor(anchor: ScrollAnchor | null): boolean {
    if (!anchor) return false
    const element = messageElements().find((candidate) => candidate.dataset.chatMessageId === anchor.messageId)
    if (!element) return false
    const topDelta = element.getBoundingClientRect().top - anchor.top
    if (topDelta !== 0) window.scrollTo({ top: window.scrollY + topDelta })
    return true
  }

  async function loadMoreHistoryFromTop(): Promise<void> {
    topHistoryLoadInFlight = true
    const anchor = captureScrollAnchor()
    const previousScrollHeight = document.documentElement.scrollHeight
    const previousScrollY = window.scrollY
    try {
      await onLoadMoreHistory?.()
      await tick()
      const restoredAnchor = restoreScrollAnchor(anchor)
      const heightDelta = document.documentElement.scrollHeight - previousScrollHeight
      if (!restoredAnchor && heightDelta > 0) window.scrollTo({ top: previousScrollY + heightDelta })
    } finally {
      topHistoryLoadInFlight = false
    }
  }

</script>

<Conversation.Root class="h-auto min-h-0 flex-1 overflow-visible">
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
    <Conversation.Content bind:ref={scrollContainer} data-chat-conversation-content class="overflow-visible px-0 py-4 sm:p-4">
      {#if hasMoreHistory && historyLoading}
        <div class="pb-2 text-center text-xs text-muted-foreground" role="status" aria-live="polite">Loading earlier messages…</div>
      {/if}
      {#each displayItems as displayItem (displayItem.id)}
        {#if displayItem.kind === 'agent_status'}
          <Message.Root from="assistant" data-chat-agent-status>
            <Message.Content>
              <AgentStatus state={sessionState} />
            </Message.Content>
          </Message.Root>
        {:else if displayItem.kind === 'agent_exit_status'}
          <AgentExitStatus state={sessionState} />
        {:else}
          {@const chatMessage = displayItem.message}
          <Message.Root from={chatMessage.role} data-chat-message-id={chatMessage.id}>
            <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
              {#if displayItem.showAgentStatus}
                <AgentStatus state={sessionState} />
              {/if}
              {#if chatMessage.role === 'assistant' && chatMessage.thoughtSteps?.length}
                <ThoughtSummary class="mb-3" steps={chatMessage.thoughtSteps} active={(sessionState ? sessionState === 'busy' : true) && chatMessage.id === activeLoadingMessageId} />
              {/if}
              {#if chatMessage.content.trim()}
                <Message.Response content={chatMessage.content} markdown={chatMessage.role === 'assistant'} />
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
                {/if}
              {/if}
            </Message.Content>
          </Message.Root>

          {#if plannerTaskId && chatMessage.id === plannerDraftAnchorId}
          <section class="mx-auto w-full max-w-4xl px-4 pb-5" aria-label="Planner draft DAG action">
            <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border bg-muted/20 p-3">
              <div class="min-w-0">
                <p class="text-sm font-medium">Planner draft DAG</p>
                <p class="truncate text-xs text-muted-foreground">Task {plannerTaskId}</p>
              </div>
              {#if draftPlannerProposalLoading}
                <span class="text-sm text-muted-foreground">Loading proposal…</span>
              {:else if draftPlannerProposal}
                {@const draftWorkItems = proposalWorkItems(draftPlannerProposal)}
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  aria-label={`View draft DAG for turn ${chatMessage.turnId}`}
                  onclick={openDraftDagSheet}
                >
                  <GitBranch class="size-4" /> View draft DAG
                  <Badge variant="secondary" class="ml-1">{draftWorkItems.length} items</Badge>
                </Button>
              {:else}
                <span class="text-sm text-muted-foreground">Waiting for proposal…</span>
              {/if}
            </div>
          </section>
          {/if}
        {/if}
      {/each}
    </Conversation.Content>
  {/if}
</Conversation.Root>

<Sheet.Root bind:open={draftDagSheetOpen}>
  {#if draftPlannerProposal}
    {@const draftWorkItems = proposalWorkItems(draftPlannerProposal)}
    {@const draftEdges = proposalEdges(draftPlannerProposal)}
    <Sheet.Content class="w-[92vw] gap-0 overflow-hidden p-0 sm:max-w-4xl">
      <Sheet.Header class="border-b px-6 py-4">
        <div class="flex flex-wrap items-start justify-between gap-3 pr-10">
          <div>
            <Sheet.Title>Planner draft DAG</Sheet.Title>
            <Sheet.Description>Task {plannerTaskId} · revision {draftPlannerProposal.revision} · {draftPlannerProposal.state}</Sheet.Description>
          </div>
          <Badge variant="secondary">{draftPlannerProposal.state}</Badge>
        </div>
      </Sheet.Header>
      <div class="min-h-0 flex-1 overflow-auto px-6 py-4">
        <p class="text-sm leading-6">{draftPlannerProposal.summary}</p>
        <div class="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
          <span class="rounded-full bg-muted px-2.5 py-1">{draftWorkItems.length} work items</span>
          <span class="rounded-full bg-muted px-2.5 py-1">{draftEdges.length} dependencies</span>
        </div>

        {#if draftWorkItems.length}
          <div class="mt-4">
            <DraftDagFlow workItems={draftWorkItems} edges={draftEdges} />
          </div>
        {:else}
          <p class="mt-3 text-sm text-muted-foreground">This draft proposal does not include work items.</p>
        {/if}
      </div>
    </Sheet.Content>
  {/if}
</Sheet.Root>
