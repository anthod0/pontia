<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ChainOfThoughtContentProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  import { getChainOfThoughtContext } from './chain-of-thought-context.svelte.js'

  let { class: className, children, ref = $bindable(null), ...restProps }: ChainOfThoughtContentProps = $props()
  const context = getChainOfThoughtContext()
</script>

{#if context.open}
  <div bind:this={ref} class={cn('mt-1 space-y-2 border-l pl-3 text-popover-foreground', className)} {...restProps}>
    {@render children?.()}
  </div>
{/if}
