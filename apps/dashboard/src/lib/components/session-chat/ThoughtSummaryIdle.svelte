<script lang="ts">
  import { Lightbulb } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'

  interface Props {
    durationMs?: number
    count?: number
    class?: string
    onOpen: () => void
  }

  let { durationMs, count = 0, class: className, onOpen }: Props = $props()

  const label = $derived(durationMs === undefined
    ? `Worked for ${count} ${count === 1 ? 'step' : 'steps'}`
    : `Worked for ${formatDuration(durationMs)}`)

  function formatDuration(milliseconds: number): string {
    const totalSeconds = Math.max(1, Math.round(milliseconds / 1_000))
    const hours = Math.floor(totalSeconds / 3_600)
    const minutes = Math.floor((totalSeconds % 3_600) / 60)
    const seconds = totalSeconds % 60
    return [
      hours ? `${hours}h` : '',
      minutes ? `${minutes}m` : '',
      seconds || (!hours && !minutes) ? `${seconds}s` : '',
    ].filter(Boolean).join(' ')
  }
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
