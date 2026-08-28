<script lang="ts">
  import { RotateCcw, Trash2, X } from '@lucide/svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import type { InboxMessageView } from '../../api/types'

  interface Props {
    messages: InboxMessageView[]
    busyMessageId: string | null
    onCancel: (message: InboxMessageView) => void
    onRetry: (message: InboxMessageView) => void
    onDismiss: (message: InboxMessageView) => void
  }

  let { messages, busyMessageId, onCancel, onRetry, onDismiss }: Props = $props()
</script>

{#if messages.length}
  <section class="mb-2 overflow-hidden rounded-lg border bg-background shadow-sm" aria-labelledby="queued-messages-title">
    <div class="px-3 py-2">
      <h2 id="queued-messages-title" class="text-xs font-medium text-muted-foreground">
        {messages.length} queued message{messages.length === 1 ? '' : 's'}
      </h2>
    </div>
    <ul class="max-h-40 overflow-y-auto">
      {#each messages as message (message.message_id)}
        <li class="group flex min-w-0 items-center gap-2 px-3 py-1.5 text-sm" title={message.failure_message ?? undefined}>
          <span class="min-w-0 flex-1 truncate" title={message.input.summary}>{message.input.summary}</span>
          {#if message.state === 'pending'}
            <div class="flex shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Cancel inbox message ${message.input.summary}`} title="Cancel" onclick={() => onCancel(message)}>
                <Trash2 class="size-3.5" />
              </Button>
            </div>
          {:else if message.state === 'failed'}
            <div class="flex shrink-0 gap-1 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Remove inbox message ${message.input.summary}`} title="Remove" onclick={() => onDismiss(message)}>
                <X class="size-3.5" />
              </Button>
              <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={`Retry inbox message ${message.input.summary}`} title="Retry" onclick={() => onRetry(message)}>
                <RotateCcw class="size-3.5" />
              </Button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
