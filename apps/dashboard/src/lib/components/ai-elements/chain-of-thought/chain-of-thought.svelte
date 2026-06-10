<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ChainOfThoughtProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
    defaultOpen?: boolean
  }
</script>

<script lang="ts">
  import { setChainOfThoughtContext } from './chain-of-thought-context.svelte.js'

  let { class: className, children, ref = $bindable(null), defaultOpen = false, ...restProps }: ChainOfThoughtProps = $props()
  setChainOfThoughtContext(() => defaultOpen)
</script>

<div bind:this={ref} class={cn('not-prose w-full space-y-2 text-sm', className)} {...restProps}>
  {@render children?.()}
</div>
