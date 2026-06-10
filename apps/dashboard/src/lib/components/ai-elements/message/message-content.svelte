<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface MessageContentProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), ...restProps }: MessageContentProps = $props()
</script>

<div
  bind:this={ref}
  class={cn(
    'max-w-full break-words text-sm leading-relaxed',
    'group-[.is-user]:rounded-2xl group-[.is-user]:bg-primary group-[.is-user]:px-4 group-[.is-user]:py-3 group-[.is-user]:text-primary-foreground',
    'group-[.is-assistant]:rounded-2xl group-[.is-assistant]:bg-transparent group-[.is-assistant]:px-4 group-[.is-assistant]:py-3 group-[.is-assistant]:text-card-foreground',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</div>
