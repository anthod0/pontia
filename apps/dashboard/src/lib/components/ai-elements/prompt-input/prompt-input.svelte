<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface PromptInputProps extends WithElementRef<HTMLAttributes<HTMLFormElement>, HTMLFormElement> {
    children?: Snippet
    onSubmit?: () => void
  }
</script>

<script lang="ts">
  let { class: className, children, onSubmit, ref = $bindable(null), ...restProps }: PromptInputProps = $props()

  function handleSubmit(event: SubmitEvent) {
    event.preventDefault()
    onSubmit?.()
  }
</script>

<form bind:this={ref} class={cn('border bg-background focus-within:border-primary', className)} onsubmit={handleSubmit} {...restProps}>
  {@render children?.()}
</form>
