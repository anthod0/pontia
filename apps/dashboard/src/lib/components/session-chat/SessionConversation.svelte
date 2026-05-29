<script lang="ts">
  import { tick } from 'svelte'
  import { Bot, UserRound } from '@lucide/svelte'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { chatAutoScrollKey, scrollToBottom } from '../../session-chat/autoScroll'
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

  function stringField(value: JsonObject, key: string, fallback = '—'): string {
    const field = value[key]
    return typeof field === 'string' && field.trim() ? field : fallback
  }
</script>

<Conversation.Root class="min-h-0 flex-1">
  {#if loading}
    <Conversation.EmptyState title="Loading conversation…" description="Fetching the latest session turns." />
  {:else if !messages.length}
    <Empty.Root class="h-full">
      <Empty.Header>
        <Empty.Media><Bot class="size-6" /></Empty.Media>
        <Empty.Title>No messages yet</Empty.Title>
        <Empty.Description>This session has no turn history yet.</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <Conversation.Content bind:ref={scrollContainer}>
      {#each messages as chatMessage (chatMessage.id)}
        <Message.Root from={chatMessage.role}>
          <div class="mb-1 flex items-center gap-2 text-xs text-muted-foreground {chatMessage.role === 'user' ? 'justify-end' : 'justify-start'}">
            {#if chatMessage.role === 'assistant'}<Bot class="size-3.5" />{:else}<UserRound class="size-3.5" />{/if}
            <span>{chatMessage.role === 'assistant' ? 'AI' : 'You'}</span>
            {#if chatMessage.status !== 'sent'}<Badge variant="secondary">{chatMessage.status}</Badge>{/if}
          </div>
          <Message.Content class={chatMessage.status === 'failed' ? 'border-destructive/40 text-destructive' : ''}>
            <Message.Response content={chatMessage.content} />
          </Message.Content>
        </Message.Root>

        {#if plannerTaskId && chatMessage.id === plannerDraftAnchorId}
          <div class="mx-auto w-full max-w-4xl px-4 pb-4">
            <div class="rounded-xl border bg-card p-4 shadow-sm">
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h3 class="text-lg font-semibold">Planner draft DAG</h3>
                  <p class="text-sm text-muted-foreground">Task {plannerTaskId}</p>
                </div>
                {#if draftPlannerProposalLoading}
                  <span class="text-sm text-muted-foreground">Loading proposal…</span>
                {:else if draftPlannerProposal}
                  <span class="rounded-full border px-2.5 py-1 text-xs text-muted-foreground">revision {draftPlannerProposal.revision} · {draftPlannerProposal.state}</span>
                {/if}
              </div>

              {#if draftPlannerProposal}
                {@const draftWorkItems = proposalWorkItems(draftPlannerProposal)}
                {@const draftEdges = proposalEdges(draftPlannerProposal)}
                <p class="mt-3 text-sm">{draftPlannerProposal.summary}</p>
                <div class="mt-4 grid gap-3 md:grid-cols-2">
                  {#each draftWorkItems as item}
                    <div class="rounded-lg border bg-background p-3">
                      <div class="font-medium">{stringField(item, 'title')}</div>
                      <div class="mt-1 text-sm text-muted-foreground">{stringField(item, 'description')}</div>
                      <div class="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                        <span>{stringField(item, 'temp_id', stringField(item, 'work_item_id', 'draft'))}</span>
                        <span>{stringField(item, 'kind')}</span>
                        <span>profile {stringField(item, 'execution_profile_id')}</span>
                      </div>
                    </div>
                  {/each}
                </div>
                {#if draftEdges.length}
                  <div class="mt-4 space-y-1 text-sm text-muted-foreground">
                    {#each draftEdges as edge}
                      <div>{stringField(edge, 'from_work_item_id')} → {stringField(edge, 'to_work_item_id')} <span class="text-xs">{stringField(edge, 'edge_type', 'depends_on')}</span></div>
                    {/each}
                  </div>
                {/if}
              {:else if !draftPlannerProposalLoading}
                <p class="mt-3 text-sm text-muted-foreground">Waiting for the planner to submit a draft DAG proposal.</p>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    </Conversation.Content>
  {/if}
</Conversation.Root>
