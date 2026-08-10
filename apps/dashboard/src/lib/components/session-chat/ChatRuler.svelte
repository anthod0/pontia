<script lang="ts">
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
  const navigableTurnIdSet = $derived(new Set(navigableTurnIds))
  const orderedTurns = $derived(turns.slice().sort(compareTurns))
  const visibleTurns = $derived(orderedTurns.filter((turn) => navigableTurnIdSet.has(turn.turn_id)))
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

{#if visibleTurns.length}
  <Tooltip.Provider delayDuration={120}>
  <aside
    class="fixed right-4 top-1/2 z-30 hidden -translate-y-1/2 xl:block"
    aria-label="Conversation ruler"
    data-chat-ruler
  >
    <div class="relative max-h-[calc(100svh-14rem)] overflow-y-auto px-2 py-2">
      <ol class="relative flex min-w-8 flex-col items-end gap-0">
        {#each visibleTurns as turn (turn.turn_id)}
          {@const branched = branchedTurnIdSet.has(turn.turn_id)}
          {#each MESSAGE_ROLES as role (role)}
            {@const summary = summaryFor(turn, role)}
            {@const branchMark = branched && role === 'user'}
            <li class="relative flex h-2 w-9 items-center justify-end">
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <button
                      type="button"
                      {...props}
                      class="group relative z-10 flex h-full w-full cursor-pointer items-center justify-end focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                      aria-label={`${roleLabel(role)} message: ${summary}`}
                      data-chat-ruler-mark
                      data-turn-id={turn.turn_id}
                      data-role={role}
                      onclick={() => activate(turn, role)}
                    >
                      <span
                        class={`h-px rounded-full transition-[width,background-color] ${role === 'user' ? 'w-[10px]' : 'w-[5px]'} ${branchMark ? 'bg-gray-500 group-hover:bg-gray-600' : 'bg-gray-300 group-hover:bg-gray-400'}`}
                        aria-hidden="true"
                        data-chat-ruler-line
                        data-chat-ruler-branch={branchMark ? 'true' : undefined}
                      ></span>
                    </button>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content
                  side="left"
                  sideOffset={8}
                  class="max-w-80 whitespace-normal border border-gray-200 bg-gray-100 text-gray-900 shadow-md"
                  arrowClasses="hidden"
                >
                  <div class="flex flex-col items-start gap-1.5">
                    <span class="font-medium">{roleLabel(role)}</span>
                    <span class="line-clamp-3 text-gray-600">{summary}</span>
                  </div>
                </Tooltip.Content>
              </Tooltip.Root>
            </li>
          {/each}
        {/each}
      </ol>
    </div>
  </aside>
  </Tooltip.Provider>
{/if}
