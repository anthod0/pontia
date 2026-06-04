<script lang="ts">
  import { tick } from 'svelte'
  import { Bot, UserRound } from '@lucide/svelte'
  import * as Conversation from '$lib/components/ai-elements/conversation/index.js'
  import * as Message from '$lib/components/ai-elements/message/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { chatAutoScrollKey, scrollToBottom } from '../../session-chat/autoScroll'
  import { buildDraftDagOutline } from '../../session-chat/draftDagOutline'
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

<Conversation.Root class="h-auto min-h-0 flex-1 overflow-visible">
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
          </Message.Content>
        </Message.Root>

        {#if plannerTaskId && chatMessage.id === plannerDraftAnchorId}
          <section class="mx-auto w-full max-w-4xl px-4 pb-5" aria-label="Planner draft DAG">
            <div class="border-y bg-muted/20 py-4">
              <div class="flex flex-wrap items-start justify-between gap-3 px-1">
                <div>
                  <h3 class="text-base font-semibold tracking-tight">Planner draft DAG</h3>
                  <p class="text-xs text-muted-foreground">Task {plannerTaskId}</p>
                </div>
                {#if draftPlannerProposalLoading}
                  <span class="text-sm text-muted-foreground">Loading proposal…</span>
                {:else if draftPlannerProposal}
                  <span class="rounded-full border bg-background px-2.5 py-1 text-xs text-muted-foreground">revision {draftPlannerProposal.revision} · {draftPlannerProposal.state}</span>
                {/if}
              </div>

              {#if draftPlannerProposal}
                {@const draftWorkItems = proposalWorkItems(draftPlannerProposal)}
                {@const draftEdges = proposalEdges(draftPlannerProposal)}
                {@const outline = buildDraftDagOutline({ workItems: draftWorkItems, edges: draftEdges })}
                <div class="mt-3 px-1">
                  <p class="text-sm leading-6">{draftPlannerProposal.summary}</p>
                  <div class="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <span class="rounded-full bg-background px-2.5 py-1">{outline.totalWorkItems} work items</span>
                    <span class="rounded-full bg-background px-2.5 py-1">{outline.totalEdges} dependencies</span>
                    <span class="rounded-full bg-background px-2.5 py-1">{outline.components.length} flows</span>
                    <span class="rounded-full bg-background px-2.5 py-1">roots {outline.rootIds.join(', ') || '—'}</span>
                    <span class="rounded-full bg-background px-2.5 py-1">leaves {outline.leafIds.join(', ') || '—'}</span>
                  </div>
                </div>

                {#if outline.warnings.length}
                  <div class="mt-3 space-y-1 px-1 text-xs text-amber-600 dark:text-amber-400">
                    {#each outline.warnings as warning}
                      <p>{warning}</p>
                    {/each}
                  </div>
                {/if}

                <div class="mt-4 space-y-4 px-1">
                  {#each outline.components as component, componentIndex}
                    <div class="space-y-2">
                      <div class="flex flex-wrap items-baseline gap-2">
                        <h4 class="text-sm font-medium">Flow {componentIndex + 1}</h4>
                        <code class="text-xs text-muted-foreground">{component.compactText}</code>
                      </div>
                      <div class="space-y-2 border-l pl-3">
                        {#each component.layers as layer, layerIndex}
                          <div class="grid gap-2 sm:grid-cols-[4.5rem_1fr]">
                            <div class="pt-1 text-xs uppercase tracking-wide text-muted-foreground">Layer {layerIndex + 1}</div>
                            <div class="flex flex-wrap gap-2">
                              {#each layer as item}
                                <span class="inline-flex max-w-full items-center gap-2 rounded-full border bg-background px-3 py-1.5 text-sm">
                                  <span class="font-mono text-xs text-muted-foreground">{item.id}</span>
                                  <span class="truncate font-medium">{item.title}</span>
                                </span>
                              {/each}
                            </div>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/each}
                </div>

                <div class="mt-4 divide-y border-y text-sm">
                  <details class="group py-2" open>
                    <summary class="cursor-pointer list-none px-1 font-medium marker:hidden">Work item details</summary>
                    <div class="mt-2 divide-y">
                      {#each outline.components as component}
                        {#each component.layers as layer}
                          {#each layer as item}
                            <div class="grid gap-1 px-1 py-2 sm:grid-cols-[10rem_1fr]">
                              <div class="font-mono text-xs text-muted-foreground">{item.id}</div>
                              <div>
                                <div class="font-medium">{item.title}</div>
                                {#if item.description}<div class="mt-0.5 text-xs text-muted-foreground">{item.description}</div>{/if}
                                <div class="mt-1 flex flex-wrap gap-2 text-xs text-muted-foreground">
                                  <span>{item.kind}</span>
                                  <span>profile {item.executionProfileId}</span>
                                  <span>priority {item.priority}</span>
                                  {#if item.optional}<span>optional</span>{/if}
                                  {#if item.parallelizable}<span>parallelizable</span>{/if}
                                </div>
                              </div>
                            </div>
                          {/each}
                        {/each}
                      {/each}
                    </div>
                  </details>
                  <details class="group py-2">
                    <summary class="cursor-pointer list-none px-1 font-medium marker:hidden">Dependency details</summary>
                    {#if outline.dependencies.length}
                      <div class="mt-2 space-y-1 px-1 text-sm text-muted-foreground">
                        {#each outline.dependencies as dependency}
                          <div class={dependency.resolved ? '' : 'text-amber-600 dark:text-amber-400'}>{dependency.label} <span class="text-xs">{dependency.edgeType}</span></div>
                        {/each}
                      </div>
                    {:else}
                      <p class="mt-2 px-1 text-sm text-muted-foreground">No dependencies in this draft.</p>
                    {/if}
                  </details>
                </div>
              {:else if !draftPlannerProposalLoading}
                <p class="mt-3 px-1 text-sm text-muted-foreground">Waiting for the planner to submit a draft DAG proposal.</p>
              {/if}
            </div>
          </section>
        {/if}
      {/each}
    </Conversation.Content>
  {/if}
</Conversation.Root>
