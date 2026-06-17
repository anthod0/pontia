<script lang="ts">
  import { Brain } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'

  interface Props {
    count: number
    class?: string
    onOpen: () => void
  }

  let { count, class: className, onOpen }: Props = $props()
  const label = $derived(`Thought for ${count} ${count === 1 ? 'step' : 'steps'}`)
</script>

<button
  type="button"
  class={cn(
    'not-prose inline-flex items-center gap-2 rounded-xl px-3 py-2.5 text-left text-muted-foreground transition hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    className,
  )}
  aria-label="View thought details"
  onclick={onOpen}
>
  <Brain class="size-5 shrink-0" aria-hidden="true" />
  <span class="text-xs leading-4">{label}</span>
</button>
