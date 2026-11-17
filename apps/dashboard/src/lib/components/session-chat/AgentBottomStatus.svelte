<script lang="ts">
  import { t } from '$lib/i18n';
  import { cn } from '$lib/utils.js'

  interface Props {
    state: string | null
    class?: string
  }

  let { state, class: className }: Props = $props()

  const statusText = $derived(
    state === 'starting'
      ? $t('chat.sessionStarting')
      : state === 'exited'
        ? $t('chat.resumeHint')
        : state === 'interrupted'
          ? $t('chat.interrupted')
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
