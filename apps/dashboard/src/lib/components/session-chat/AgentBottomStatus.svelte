<script lang="ts">
  import { cn } from '$lib/utils.js'

  interface Props {
    state: string | null
    class?: string
  }

  let { state, class: className }: Props = $props()

  const statusText = $derived(
    state === 'starting'
      ? 'Session starting'
      : state === 'exited'
        ? 'session exited · send a message to resume'
        : state === 'interrupted'
          ? 'session interrupted'
          : null,
  )
</script>

{#if statusText}
  <div
    class={cn('not-prose flex justify-start py-4 text-xs text-muted-foreground', className)}
    data-chat-session-bottom-status
    role="status"
    aria-label={statusText}
  >
    <span>{statusText}</span>
  </div>
{/if}
