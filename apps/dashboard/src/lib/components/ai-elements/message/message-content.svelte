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
    'min-w-0 max-w-full break-words text-[13.5px] leading-relaxed',
    'group-[.is-user]:rounded-lg group-[.is-user]:border group-[.is-user]:border-border group-[.is-user]:bg-muted group-[.is-user]:px-4 group-[.is-user]:py-3 group-[.is-user]:text-heading',
    'group-[.is-assistant]:w-full group-[.is-assistant]:bg-transparent group-[.is-assistant]:text-card-foreground',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</div>
