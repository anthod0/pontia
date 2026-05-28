<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface MessageResponseProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    content: string
  }
</script>

<script lang="ts">
  let { content, class: className, ref = $bindable(null), ...restProps }: MessageResponseProps = $props()
</script>

<div bind:this={ref} class={cn('size-full [&>*:first-child]:mt-0 [&>*:last-child]:mb-0', className)} {...restProps}>
  {content}
</div>
