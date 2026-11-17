<script lang="ts">
  import { t } from '$lib/i18n';
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import type { InboxMessageView } from '../../api/types'

  interface Props {
    sessionId: string
    messages: InboxMessageView[]
    busyMessageId: string | null
    onCancel: (message: InboxMessageView) => void
    onRetry: (message: InboxMessageView) => void
    onDismiss: (message: InboxMessageView) => void
  }

  let { sessionId, messages, busyMessageId, onCancel, onRetry, onDismiss }: Props = $props()
  const labels = $derived({ pending: $t('common.waiting'), failed: $t('common.failed') } as Record<string, string>)
  let visibleMessages = $derived(messages.filter((message) =>
    message.session_id === sessionId && !message.retried_by_message_id &&
    (message.state === 'pending' || message.state === 'failed')))
</script>

{#if visibleMessages.length}
  <section class="mb-2 overflow-hidden rounded-lg border bg-background shadow-none" aria-label={$t('inbox.title')}>
    <ul class="max-h-40 overflow-y-auto py-1">
      {#each visibleMessages as message (message.message_id)}
        <li class="group flex min-w-0 items-center gap-2 px-3 py-1.5 text-sm" title={message.failure_message ?? undefined}>
          <div class="min-w-0 flex-1">
            <p class="truncate" title={message.input.summary}>{message.input.summary}</p>
            <p class="text-xs text-muted-foreground">{labels[message.state] ?? message.state}{message.retry_of_message_id ? $t('inbox.retrySuffix') : ''}</p>
            {#if message.failure_message}<p class="text-xs text-destructive">{message.failure_message}</p>{/if}
            {#if message.retry_of_message_id}<p class="truncate text-xs text-muted-foreground" title={message.retry_of_message_id}>{$t('inbox.retryOf')} {message.retry_of_message_id}</p>{/if}
          </div>
          {#if message.state === 'pending'}
            <div class="flex shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={$t('inbox.cancelMessage', { summary: message.input.summary })} title={$t('common.cancel')} onclick={() => onCancel(message)}>
                <TrashIcon class="size-3.5" />
              </Button>
            </div>
          {:else if message.state === 'failed'}
            <div class="flex shrink-0 gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={$t('inbox.removeMessage', { summary: message.input.summary })} title={$t('common.remove')} onclick={() => onDismiss(message)}>
                <XIcon class="size-3.5" />
              </Button>
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={$t('inbox.retryMessage', { summary: message.input.summary })} title={$t('common.retry')} onclick={() => onRetry(message)}>
                <ArrowCounterClockwiseIcon class="size-3.5" />
              </Button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
