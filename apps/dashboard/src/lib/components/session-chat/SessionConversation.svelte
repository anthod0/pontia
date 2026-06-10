<script lang="ts">
  import { tick } from 'svelte'
  import { Bot, GitBranch, UserRound } from '@lucide/svelte'
  import * as ChainOfThought from '$lib/components/ai-elements/chain-of-thought/index.js'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { chatAutoScrollKey, scrollToBottom } from '../../session-chat/autoScroll'
  import DraftDagFlow from '../../../components/dag/DraftDagFlow.svelte'
  import type { DagProposalView, JsonObject } from '../../../api/types'
  import type { SessionChatMessage } from '../../session-chat/sessionChat'

  interface Props {
    messages: SessionChatMessage[]
    loading?: boolean
    plannerTaskId?: string | null
    draftPlannerProposal?: DagProposalView | null
    draftPlannerProposalLoading?: boolean
  }

  let { messages, loading = false, plannerTaskId = null, draftPlannerProposal = null, draftPlannerProposalLoading = false }: Props = $props()
  let scrollContainer = $state<HTMLDivElement | null>(null)
  let draftDagSheetOpen = $state(false)
  const scrollKey = $derived(chatAutoScrollKey(messages))
  const plannerDraftAnchorId = $derived(lastAssistantMessageId(messages))

  $effect(() => {
    scrollKey
    void tick().then(() => scrollToBottom(scrollContainer))
  })

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
  {:else if !messages.length}
    <Empty.Root class="h-full">
      <Empty.Header>
        <Empty.Media><Bot class="size-6" /></Empty.Media>
        <Empty.Title>No messages yet</Empty.Title>
        <Empty.Description>This session has no turn history yet.</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Conversation.Content bind:ref={scrollContainer} class="overflow-visible">
      {#each messages as chatMessage (chatMessage.id)}
        <Message.Root from={chatMessage.role}>
          <div class="mb-1 flex items-center gap-2 text-xs text-muted-foreground {chatMessage.role === 'user' ? 'justify-end' : 'justify-start'}">
            {#if chatMessage.role === 'assistant'}<Bot class="size-3.5" />{:else}<UserRound class="size-3.5" />{/if}
            <span>{chatMessage.role === 'assistant' ? 'AI' : 'You'}</span>
            {#if chatMessage.status !== 'sent'}<Badge variant="secondary">{chatMessage.status}</Badge>{/if}
          </div>
          <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
            <Message.Response content={chatMessage.content} />
            {#if chatMessage.role === 'assistant' && chatMessage.thoughtSteps?.length}
              <ChainOfThought.Root class="mt-3">
                <ChainOfThought.Header>{chatMessage.thoughtSteps.length} thinking/tool steps</ChainOfThought.Header>
                <ChainOfThought.Content>
                  {#each chatMessage.thoughtSteps as step (step.id)}
                    <ChainOfThought.Step title={step.title} status={step.status} kind={step.kind}>{step.content}</ChainOfThought.Step>
                  {/each}
                </ChainOfThought.Content>
              </ChainOfThought.Root>
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
