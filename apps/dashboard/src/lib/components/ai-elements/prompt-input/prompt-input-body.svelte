<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface PromptInputBodyProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), ...restProps }: PromptInputBodyProps = $props()
</script>

<div bind:this={ref} class={cn('flex flex-col gap-2', className)} {...restProps}>
  {@render children?.()}
</div>
