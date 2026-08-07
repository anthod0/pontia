<script lang="ts">
  import { GitFork } from '@lucide/svelte'
  import * as Tooltip from '$lib/components/ui/tooltip/index.js'
  import type { TurnView } from '../../../api/types'
  import type { ChatMessageRole } from '../../session-chat/sessionChat'

  interface Props {
    turns: TurnView[]
    treeMode?: boolean
    navigableTurnIds?: string[]
    onNavigate?: (turnId: string, role: ChatMessageRole) => void | Promise<void>
  }

  let {
    turns,
    treeMode = false,
    navigableTurnIds = [],
    onNavigate,
  }: Props = $props()

  const MESSAGE_ROLES: ChatMessageRole[] = ['user', 'assistant']
  const orderedTurns = $derived(turns.slice().sort(compareTurns))
  const navigableTurnIdSet = $derived(new Set(navigableTurnIds))
  const branchedTurnIdSet = $derived(branchTurnIds(orderedTurns, treeMode))

  function compareTurns(a: TurnView, b: TurnView): number {
    const timeComparison = a.created_at.localeCompare(b.created_at)
    return timeComparison || a.turn_id.localeCompare(b.turn_id)
  }

  function branchTurnIds(items: TurnView[], enabled: boolean): Set<string> {
    if (!enabled) return new Set()
    const childrenByParent = new Map<string, TurnView[]>()
    for (const turn of items) {
      if (turn.topology_status !== 'linked' || !turn.parent_turn_id) continue
      const children = childrenByParent.get(turn.parent_turn_id) ?? []
      children.push(turn)
      childrenByParent.set(turn.parent_turn_id, children)
    }
    return new Set(
      [...childrenByParent.values()]
        .filter((children) => children.length > 1)
        .flatMap((children) => children.map((turn) => turn.turn_id)),
    )
  }

  function summaryFor(turn: TurnView, role: ChatMessageRole): string {
    const value = role === 'user' ? turn.input?.summary : turn.output?.summary
    if (typeof value === 'string' && value.trim()) return value.trim()
    if (role === 'assistant' && turn.failure) {
      if (typeof turn.failure === 'string' && turn.failure.trim()) return turn.failure.trim()
      if (typeof turn.failure === 'object') {
        const message = (turn.failure as { message?: unknown }).message
        if (typeof message === 'string' && message.trim()) return message.trim()
      }
    }
    return role === 'user' ? 'No user summary available.' : 'No assistant summary available.'
  }

  function roleLabel(role: ChatMessageRole): string {
    return role === 'user' ? 'User' : 'Assistant'
  }

  function activate(turn: TurnView, role: ChatMessageRole): void {
    if (!navigableTurnIdSet.has(turn.turn_id)) return
    void onNavigate?.(turn.turn_id, role)
  }
</script>

{#if orderedTurns.length}
  <Tooltip.Provider delayDuration={120}>
  <aside
    class="fixed right-4 top-1/2 z-30 hidden -translate-y-1/2 xl:block"
    aria-label="Conversation ruler"
    data-chat-ruler
  >
    <div class="relative max-h-[calc(100svh-14rem)] overflow-y-auto px-2 py-2">
      <ol class="relative flex min-w-8 flex-col items-end gap-0">
        {#each orderedTurns as turn (turn.turn_id)}
          {@const navigable = navigableTurnIdSet.has(turn.turn_id)}
          {@const branched = branchedTurnIdSet.has(turn.turn_id)}
          {#each MESSAGE_ROLES as role (role)}
            {@const summary = summaryFor(turn, role)}
            <li class="relative flex h-2 w-9 items-center justify-end">
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <button
                      type="button"
                      {...props}
                      class={`relative z-10 h-px rounded-full bg-gray-300 transition-[width,background-color,opacity] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 ${role === 'user' ? 'w-6' : 'w-4'} ${navigable ? 'cursor-pointer hover:bg-gray-400' : 'cursor-default opacity-50'}`}
                      aria-label={`${roleLabel(role)} message: ${summary}${navigable ? '' : ' (not on the current branch)'}`}
                      aria-disabled={!navigable}
                      data-chat-ruler-mark
                      data-turn-id={turn.turn_id}
                      data-role={role}
                      data-navigable={navigable ? 'true' : 'false'}
                      onclick={() => activate(turn, role)}
                    ></button>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="left" sideOffset={8} class="max-w-80 whitespace-normal">
                  <span class="font-medium">{roleLabel(role)}</span>
                  <span class="line-clamp-3 text-background/80">{summary}</span>
                  {#if !navigable}
                    <span class="text-background/60">Not on the current branch</span>
                  {/if}
                </Tooltip.Content>
              </Tooltip.Root>
              {#if branched && role === 'user'}
                <GitFork
                  class="absolute -right-1 size-3 text-primary"
                  aria-label="Branch turn"
                  data-chat-ruler-branch
                  data-turn-id={turn.turn_id}
                />
              {/if}
            </li>
          {/each}
        {/each}
      </ol>
    </div>
  </aside>
  </Tooltip.Provider>
{/if}
