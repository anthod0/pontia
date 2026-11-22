<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import ImageTile from './ImageTile.svelte'
  import type { SharedFileState } from './sharedFileState.svelte'
  import { MOSAIC_TILES as MAX } from './layout'

  /**
   * Fixed layouts by count, cropped to fill: 2 side by side, 3 as one large and two small,
   * 4 as 2+2, and 5 or more as 2+3 with the hidden count on the fifth. Expanded shows a 3-column grid.
   */
  let { itemActions, states, expanded = $bindable(false), highlighted = null, onenlarge, itemProps }: {
    itemActions?: Snippet
    states: SharedFileState[]
    expanded?: boolean
    highlighted?: string | null
    onenlarge: (state: SharedFileState) => void
    itemProps: (state: SharedFileState) => HTMLAttributes<HTMLDivElement>
  } = $props()
  const more = $derived(expanded ? 0 : Math.max(0, states.length - MAX))
  const rows = $derived.by(() => {
    if (expanded) return null
    const n = states.length
    if (n === 2) return [{ tiles: states, class: 'grid-cols-2 aspect-[2/1]' }]
    if (n === 4) return [{ tiles: states.slice(0, 2), class: 'grid-cols-2 aspect-[2/1]' }, { tiles: states.slice(2), class: 'grid-cols-2 aspect-[2/1]' }]
    return [{ tiles: states.slice(0, 2), class: 'grid-cols-2 aspect-[2/1]' }, { tiles: states.slice(2, MAX), class: 'grid-cols-3 aspect-[3/1]' }]
  })
</script>

{#snippet tile(state: SharedFileState, className: string, extra = 0)}
  <ImageTile {itemActions} {state} class={className} more={extra} highlighted={highlighted === state.file.path} onenlarge={() => onenlarge(state)} onmore={() => expanded = true} itemProps={itemProps(state)} />
{/snippet}

{#if expanded}
  <div class="grid grid-cols-3 gap-0.5">
    {#each states as state (state.file.path)}{@render tile(state, 'aspect-square')}{/each}
  </div>
  {#if states.length > MAX}
    <button type="button" class="w-full pt-1.5 text-xs font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" aria-expanded="true" onclick={() => expanded = false}>{$t('files.showLess')}</button>
  {/if}
{:else if states.length === 3}
  <div class="grid aspect-[3/2] grid-cols-[2fr_1fr] grid-rows-2 gap-0.5">
    {@render tile(states[0], 'row-span-2')}
    {@render tile(states[1], '')}
    {@render tile(states[2], '')}
  </div>
{:else}
  <div class="flex flex-col gap-0.5">
    {#each rows ?? [] as row, r (r)}
      <div class={['grid gap-0.5', row.class]}>
        {#each row.tiles as state, i (state.file.path)}
          {@render tile(state, '', r === 1 && i === row.tiles.length - 1 ? more : 0)}
        {/each}
      </div>
    {/each}
  </div>
{/if}
