<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ConversationContentProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), ...restProps }: ConversationContentProps = $props()
</script>

<div bind:this={ref} class={cn('flex flex-1 flex-col gap-6 overflow-y-auto p-4', className)} {...restProps}>
  {@render children?.()}
</div>
