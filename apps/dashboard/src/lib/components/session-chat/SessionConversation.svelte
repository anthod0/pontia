<script lang="ts">
  import { tick } from 'svelte'
  import { Bot, GitBranch } from '@lucide/svelte'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { chatAutoScrollKey, scrollDocumentToBottom } from '../../session-chat/autoScroll'
  import DraftDagFlow from '../../../components/dag/DraftDagFlow.svelte'
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
  }

  let { messages, sessionState = null, loading = false, plannerTaskId = null, draftPlannerProposal = null, draftPlannerProposalLoading = false }: Props = $props()
  let scrollContainer = $state<HTMLDivElement | null>(null)
  let draftDagSheetOpen = $state(false)
  const loadingPlaceholder = $derived(assistantLoadingPlaceholder(sessionState))
  const displayMessages = $derived(messagesForDisplay(messages, loadingPlaceholder))
  const scrollKey = $derived(chatAutoScrollKey(displayMessages))
  const plannerDraftAnchorId = $derived(lastAssistantMessageId(displayMessages))

  $effect(() => {
    scrollKey
    void tick().then(scrollDocumentToBottom)
  })

  function assistantLoadingPlaceholder(state: string | null): { title: string; description: string } | null {
    if (state === 'created') return { title: 'Session created', description: 'Waiting for the agent session to start.' }
    if (state === 'starting') return { title: 'Session starting', description: 'Waiting for the agent session to become ready.' }
    if (state === 'busy') return { title: 'Agent working', description: 'Waiting for the agent to report its next output.' }
    return null
  }

  function messagesForDisplay(chatMessages: SessionChatMessage[], placeholder: { title: string; description: string } | null): SessionChatMessage[] {
    if (!placeholder || chatMessages.at(-1)?.role === 'assistant') return chatMessages
    return [
      ...chatMessages,
      {
        id: `${sessionState ?? 'session'}:assistant-loading-placeholder`,
        turnId: `${sessionState ?? 'session'}:assistant-loading-placeholder`,
        role: 'assistant',
        content: '',
        status: 'pending',
        createdAt: '',
      },
    ]
  }

  function lastAssistantMessageId(chatMessages: SessionChatMessage[]): string | null {
    for (let index = chatMessages.length - 1; index >= 0; index -= 1) {
      if (chatMessages[index]?.role === 'assistant') return chatMessages[index].id
    }
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
    <Conversation.Content bind:ref={scrollContainer} class="overflow-visible">
      {#each displayMessages as chatMessage (chatMessage.id)}
        <Message.Root from={chatMessage.role}>
          <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
            {#if chatMessage.role === 'assistant' && chatMessage.thoughtSteps?.length}
              <ThoughtSummary class="mb-3" steps={chatMessage.thoughtSteps} active={(sessionState ? sessionState === 'busy' : true) && chatMessage.status === 'pending'} />
            {/if}
            {#if chatMessage.role === 'assistant' && loadingPlaceholder && !chatMessage.content.trim()}
              <div class="max-w-md space-y-1 text-muted-foreground" aria-live="polite">
                <p class="text-sm font-medium text-foreground">{loadingPlaceholder.title}</p>
                <p class="text-xs leading-5">{loadingPlaceholder.description}</p>
              </div>
            {:else if chatMessage.content.trim()}
              <Message.Response content={chatMessage.content} markdown={chatMessage.role === 'assistant'} />
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
