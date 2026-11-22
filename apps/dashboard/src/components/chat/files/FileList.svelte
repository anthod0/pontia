<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import FileRow from './FileRow.svelte'
  import type { SharedFileState } from './sharedFileState.svelte'
  import { LIST_COLLAPSED, LIST_FULL } from './layout'

  /** Rows of a multi-file share. Long lists show their first rows until expanded. */
  let { itemActions, states, expanded = $bindable(false), highlighted = null, onenlarge, itemProps }: {
    itemActions?: Snippet
    states: SharedFileState[]
    expanded?: boolean
    highlighted?: string | null
    onenlarge: (state: SharedFileState) => void
    itemProps: (state: SharedFileState) => HTMLAttributes<HTMLDivElement>
  } = $props()
  const collapsible = $derived(states.length > LIST_FULL)
  const shown = $derived(collapsible && !expanded ? states.slice(0, LIST_COLLAPSED) : states)
</script>

<ul class="space-y-0.5">
  {#each shown as state (state.file.path)}
    {@const props = itemProps(state)}
    <li>
      <FileRow {itemActions} {state} onenlarge={state.image ? () => onenlarge(state) : undefined} {...props} data-shared-file={state.file.path}
        class={['rounded-xl p-1.5 transition-[background-color,box-shadow] hover:bg-background/60', highlighted === state.file.path && 'bg-background/60 ring-2 ring-primary/50', props.class]} />
    </li>
  {/each}
</ul>
{#if collapsible}
  <button type="button" class="mt-0.5 flex w-full items-center justify-center gap-1 rounded-xl py-1.5 text-xs font-medium text-muted-foreground outline-none hover:bg-background/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" aria-expanded={expanded} onclick={() => expanded = !expanded}>
    {expanded ? $t('files.showLess') : $t('files.showMoreFiles', { count: states.length - LIST_COLLAPSED })}
    <CaretDownIcon class={['size-3.5 transition-transform', expanded && 'rotate-180']} />
  </button>
{/if}
