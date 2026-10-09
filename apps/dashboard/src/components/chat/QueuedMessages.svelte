<script lang="ts">
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
  const labels: Record<string, string> = { pending: 'Waiting', failed: 'Failed' }
  let visibleMessages = $derived(messages.filter((message) =>
    message.session_id === sessionId && !message.retried_by_message_id &&
    (message.state === 'pending' || message.state === 'failed')))
  let waiting = $derived(visibleMessages.filter((message) => message.state === 'pending').length)
  let failed = $derived(visibleMessages.filter((message) => message.state === 'failed').length)
</script>

{#if visibleMessages.length}
  <section class="mb-2 overflow-hidden rounded-none border bg-background shadow-none" aria-labelledby="queued-messages-title">
    <div class="px-3 py-2">
      <h2 id="queued-messages-title" class="text-xs font-medium text-muted-foreground">
        Inbox · {waiting} waiting · {failed} failed
      </h2>
    </div>
    <ul class="max-h-40 overflow-y-auto">
      {#each visibleMessages as message (message.message_id)}
        <li class="group flex min-w-0 items-center gap-2 px-3 py-1.5 text-sm" title={message.failure_message ?? undefined}>
          <div class="min-w-0 flex-1">
            <p class="truncate" title={message.input.summary}>{message.input.summary}</p>
            <p class="text-xs text-muted-foreground">{labels[message.state] ?? message.state}{message.retry_of_message_id ? ' · Retry' : ''}</p>
            {#if message.failure_message}<p class="text-xs text-destructive">{message.failure_message}</p>{/if}
            {#if message.retry_of_message_id}<p class="truncate text-xs text-muted-foreground" title={message.retry_of_message_id}>Retry of {message.retry_of_message_id}</p>{/if}
          </div>
          {#if message.state === 'pending'}
            <div class="flex shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Cancel inbox message ${message.input.summary}`} title="Cancel" onclick={() => onCancel(message)}>
                <TrashIcon class="size-3.5" />
              </Button>
            </div>
          {:else if message.state === 'failed'}
            <div class="flex shrink-0 gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Remove inbox message ${message.input.summary}`} title="Remove" onclick={() => onDismiss(message)}>
                <XIcon class="size-3.5" />
              </Button>
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Retry inbox message ${message.input.summary}`} title="Retry" onclick={() => onRetry(message)}>
                <ArrowCounterClockwiseIcon class="size-3.5" />
              </Button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
