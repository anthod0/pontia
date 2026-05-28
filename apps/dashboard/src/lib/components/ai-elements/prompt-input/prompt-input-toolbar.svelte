<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface PromptInputToolbarProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), ...restProps }: PromptInputToolbarProps = $props()
</script>

<div bind:this={ref} class={cn('flex items-center justify-between gap-2 pt-2', className)} {...restProps}>
  {@render children?.()}
</div>
