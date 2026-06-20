<script lang="ts">
  import { Lightbulb } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'

  interface Props {
    count: number
    class?: string
    onOpen: () => void
  }

  let { count, class: className, onOpen }: Props = $props()

  const label = $derived(`Worked for ${count} ${count === 1 ? 'step' : 'steps'}`)
</script>

<button
  type="button"
  class={cn(
    'not-prose flex min-w-0 items-center gap-2 rounded-xl pr-3 pt-2.5 text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:text-foreground',
    className,
  )}
  aria-label="View thought details"
  onclick={onOpen}
>
  <span class="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground" aria-hidden="true">
    <Lightbulb class="size-4" />
  </span>
  <span class="truncate text-xs leading-4">{label}</span>
</button>
