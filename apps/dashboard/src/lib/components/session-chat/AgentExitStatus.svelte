<script lang="ts">
  import { cn } from '$lib/utils.js'

  interface Props {
    state: string | null
    class?: string
  }

  let { state, class: className }: Props = $props()
</script>

{#if state === 'exited'}
  <div
    class={cn('not-prose flex items-center gap-3 py-4 text-xs text-muted-foreground', className)}
    data-chat-session-terminal-status
    role="status"
    aria-label="session exited, send a message to resume"
  >
    <div class="h-px flex-1 bg-border" aria-hidden="true"></div>
    <span class="shrink-0">session exited · send a message to resume</span>
    <div class="h-px flex-1 bg-border" aria-hidden="true"></div>
  </div>
{/if}
