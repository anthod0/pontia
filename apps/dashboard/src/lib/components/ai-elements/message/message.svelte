<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export type MessageRole = 'user' | 'assistant' | 'system' | 'tool' | 'data' | 'function'

  export interface MessageProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    from: MessageRole
    children?: Snippet
  }
</script>

<script lang="ts">
  let { from, class: className, children, ref = $bindable(null), ...restProps }: MessageProps = $props()
</script>

<div
  bind:this={ref}
  class={cn('group flex w-full max-w-[95%] min-w-0 flex-col gap-2', from === 'user' ? 'is-user ml-auto items-end' : 'is-assistant items-start', className)}
  data-role={from}
  {...restProps}
>
  {@render children?.()}
</div>
