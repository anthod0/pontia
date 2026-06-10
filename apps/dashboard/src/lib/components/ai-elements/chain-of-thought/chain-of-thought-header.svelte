<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import { ChevronDown } from '@lucide/svelte'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ChainOfThoughtHeaderProps extends WithElementRef<HTMLAttributes<HTMLButtonElement>, HTMLButtonElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  import { getChainOfThoughtContext } from './chain-of-thought-context.svelte.js'

  let { class: className, children, ref = $bindable(null), ...restProps }: ChainOfThoughtHeaderProps = $props()
  const context = getChainOfThoughtContext()
</script>

<button
  bind:this={ref}
  type="button"
  class={cn('flex w-full items-center gap-2 rounded-lg px-1 py-1 text-left text-xs font-medium text-muted-foreground transition hover:text-foreground', className)}
  aria-expanded={context.open}
  onclick={() => context.toggle()}
  {...restProps}
>
  <ChevronDown class={cn('size-3.5 transition-transform', context.open ? '' : '-rotate-90')} />
  <span class="min-w-0 flex-1">{@render children?.()}</span>
</button>
