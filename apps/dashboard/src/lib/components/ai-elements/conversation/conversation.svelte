<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ConversationProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), ...restProps }: ConversationProps = $props()
</script>

<div bind:this={ref} class={cn('relative flex h-full flex-col overflow-hidden', className)} role="log" {...restProps}>
  {@render children?.()}
</div>
