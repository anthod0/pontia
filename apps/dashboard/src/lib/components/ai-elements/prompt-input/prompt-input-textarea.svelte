<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { HTMLTextareaAttributes } from 'svelte/elements'

  export interface PromptInputTextareaProps extends WithElementRef<HTMLTextareaAttributes, HTMLTextAreaElement> {
    value?: string
  }
</script>

<script lang="ts">
  let { class: className, value = $bindable(''), ref = $bindable(null), ...restProps }: PromptInputTextareaProps = $props()
</script>

<textarea
  bind:this={ref}
  bind:value
  class={cn('block field-sizing-content max-h-48 min-h-10 w-full resize-none overflow-y-auto border-0 bg-transparent px-2 py-2 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50', className)}
  {...restProps}
></textarea>
