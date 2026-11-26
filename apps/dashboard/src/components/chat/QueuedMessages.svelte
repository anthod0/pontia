<script lang="ts">
  import { parseUploadMessage } from "$lib/uploads/message"
  import { t } from '$lib/i18n';
  import RetryMessageButton from './RetryMessageButton.svelte'
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
  let visibleMessages = $derived(messages.filter((message) =>
    message.session_id === sessionId && !message.retried_by_message_id &&
    (message.state === 'pending' || message.state === 'failed')))
</script>

{#if visibleMessages.length}
  <section class="mb-2 overflow-hidden rounded-item border bg-background shadow-none" aria-label={$t('inbox.title')}>
    <ul class="max-h-40 overflow-y-auto py-1">
      {#each visibleMessages as message (message.message_id)}
        {@const parts = parseUploadMessage(message.input.summary)}
        {@const summary = parts.text || [...parts.files.map(file => file.name), ...parts.secrets.map(secret => secret.label || secret.masked)].join(', ')}
        <li class="group flex min-w-0 items-center gap-2 px-3 py-1.5 text-sm">
          <div class="min-w-0 flex-1">
            <p class="truncate" title={summary}>{summary}</p>
            {#if message.state === 'pending'}<p class="text-xs text-muted-foreground">{$t('common.waiting')}{message.retry_of_message_id ? $t('inbox.retrySuffix') : ''}</p>{/if}
            {#if message.retry_of_message_id}<p class="truncate text-xs text-muted-foreground" title={message.retry_of_message_id}>{$t('inbox.retryOf')} {message.retry_of_message_id}</p>{/if}
          </div>
          {#if message.state === 'pending'}
            <div class="flex shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={$t('inbox.cancelMessage', { summary: summary })} title={$t('common.cancel')} onclick={() => onCancel(message)}>
                <TrashIcon class="size-3.5" />
              </Button>
            </div>
          {:else if message.state === 'failed'}
            <RetryMessageButton disabled={busyMessageId === message.message_id}
              label={$t('inbox.retryMessage', { summary: summary })}
              reason={message.failure_message} onclick={() => onRetry(message)} />
            <Button variant="ghost" size="icon-xs" class="sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100" disabled={busyMessageId === message.message_id} aria-label={$t('inbox.removeMessage', { summary: summary })} title={$t('common.remove')} onclick={() => onDismiss(message)}>
              <XIcon class="size-3.5" />
            </Button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
