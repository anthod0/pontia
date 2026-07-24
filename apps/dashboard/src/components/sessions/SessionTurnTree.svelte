<script lang="ts">
  import { ChevronDown, CircleAlert, GitBranch } from '@lucide/svelte'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import type { TurnView } from '../../api/types'
  import { formatDateTime, jsonPreview, shortId } from '../tasks/format'
  import {
    buildSessionTurnTree,
    type SessionTurnTreeRelation,
  } from '../../pages/sessions/sessionTurnTree'

  interface Props {
    turns: TurnView[]
    currentTurnId: string | null
  }

  let { turns, currentTurnId }: Props = $props()
  const rows = $derived(buildSessionTurnTree(turns, currentTurnId))
  const issueCount = $derived(rows.filter((row) => row.relation === 'unknown' || row.relation === 'orphan' || row.relation === 'cycle').length)

  function inputSummary(turn: TurnView): string {
    return turn.input?.summary?.trim() || jsonPreview(turn.input)
  }

  function outputDetail(turn: TurnView): string {
    if (turn.output) return jsonPreview(turn.output)
    if (turn.failure) return jsonPreview(turn.failure)
    return 'No output reported.'
  }

  function relationLabel(relation: SessionTurnTreeRelation): string {
    switch (relation) {
      case 'root': return 'Root'
      case 'linked': return 'Linked'
      case 'unknown': return 'Unknown topology'
      case 'orphan': return 'Missing parent'
      case 'cycle': return 'Cycle detected'
    }
  }
</script>

{#if rows.length}
  <div class="space-y-3">
    <div class="flex flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground">
      <span>{rows.length} turn{rows.length === 1 ? '' : 's'} across the complete reported topology.</span>
      {#if issueCount}
        <Badge variant="destructive"><CircleAlert class="size-3" /> {issueCount} topology issue{issueCount === 1 ? '' : 's'}</Badge>
      {/if}
    </div>

    <div class="overflow-x-auto rounded-lg border bg-muted/20 p-3 sm:p-4" data-session-turn-tree>
      <div class="min-w-[22rem] space-y-2">
        {#each rows as row (row.turn.turn_id)}
          <div class="relative" style={`padding-left: ${row.depth * 1.5}rem`}>
            {#if row.depth > 0}
              <span
                aria-hidden="true"
                class="absolute top-0 h-6 w-3 border-b border-l border-border"
                style={`left: ${(row.depth * 1.5) - 0.75}rem`}
              ></span>
            {/if}
            <Collapsible.Root>
              <article class={`overflow-hidden rounded-lg border bg-background shadow-xs ${row.isCurrent ? 'border-primary ring-1 ring-primary/30' : row.isCurrentBranch ? 'border-primary/40 bg-primary/5' : ''}`}>
                <Collapsible.Trigger class="group flex w-full items-start gap-3 p-3 text-left hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset">
                  <div class={`mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full border ${row.isCurrentBranch ? 'border-primary bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'}`}>
                    <GitBranch class="size-3.5" />
                  </div>
                  <div class="min-w-0 flex-1 space-y-1.5">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="font-mono text-xs font-semibold">{shortId(row.turn.turn_id)}</span>
                      <Badge variant="secondary">{row.turn.state}</Badge>
                      {#if row.isCurrent}<Badge>Current</Badge>{/if}
                      {#if row.relation !== 'linked'}
                        <Badge variant={row.relation === 'root' ? 'outline' : 'destructive'}>{relationLabel(row.relation)}</Badge>
                      {/if}
                      {#if row.childCount > 1}<Badge variant="outline">{row.childCount} branches</Badge>{/if}
                    </div>
                    <p class="truncate text-sm font-medium" title={inputSummary(row.turn)}>{inputSummary(row.turn)}</p>
                    <p class="text-xs text-muted-foreground">{formatDateTime(row.turn.created_at)}</p>
                  </div>
                  <ChevronDown class="mt-1 size-4 shrink-0 text-muted-foreground transition-transform group-data-[state=open]:rotate-180" />
                </Collapsible.Trigger>
                <Collapsible.Content>
                  <div class="grid gap-3 border-t bg-muted/10 p-3 text-sm md:grid-cols-2">
                    <div class="min-w-0 space-y-1">
                      <div class="text-xs font-medium uppercase tracking-wide text-muted-foreground">Input</div>
                      <pre class="whitespace-pre-wrap break-words font-sans">{jsonPreview(row.turn.input)}</pre>
                    </div>
                    <div class="min-w-0 space-y-1">
                      <div class="text-xs font-medium uppercase tracking-wide text-muted-foreground">{row.turn.failure ? 'Failure' : 'Output'}</div>
                      <pre class={`whitespace-pre-wrap break-words font-sans ${row.turn.failure ? 'text-destructive' : ''}`}>{outputDetail(row.turn)}</pre>
                    </div>
                    <div class="text-xs text-muted-foreground md:col-span-2">
                      Parent: <span class="font-mono">{shortId(row.turn.parent_turn_id)}</span>
                      · topology: {row.turn.topology_status}
                      · completed: {formatDateTime(row.turn.completed_at)}
                    </div>
                  </div>
                </Collapsible.Content>
              </article>
            </Collapsible.Root>
          </div>
        {/each}
      </div>
    </div>
  </div>
{:else}
  <Empty.Root>
    <Empty.Header>
      <Empty.Title>No turn topology</Empty.Title>
      <Empty.Description>This session has no reported turns to display.</Empty.Description>
    </Empty.Header>
  </Empty.Root>
{/if}
