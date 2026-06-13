<script lang="ts">
  import { RotateCcw, X } from '@lucide/svelte'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import type { InboxMessageView } from '../../api/types'
  import { formatDateTime, shortId } from '../tasks/format'
  import { inboxBadgeVariant } from './sessionMetadata'

  interface Props {
    open: boolean
    inboxActionableCount: number
    visibleInboxMessages: InboxMessageView[]
    busyMessageId: string | null
    onCancel: (message: InboxMessageView) => void
    onRetry: (message: InboxMessageView) => void
    onDismiss: (message: InboxMessageView) => void
  }

  let { open = $bindable(false), inboxActionableCount, visibleInboxMessages, busyMessageId, onCancel, onRetry, onDismiss }: Props = $props()
</script>

<Sheet.Root bind:open>
  <Sheet.Content class="w-[92vw] gap-0 overflow-hidden p-0 sm:max-w-xl">
    <Sheet.Header class="border-b px-6 py-4">
      <Sheet.Title>Inbox</Sheet.Title>
      <Sheet.Description>{inboxActionableCount} message{inboxActionableCount === 1 ? '' : 's'} · follow-up input queue</Sheet.Description>
    </Sheet.Header>
    <div class="max-h-[calc(100vh-7rem)] overflow-y-auto p-6">
      {#if visibleInboxMessages.length}
        <div class="space-y-3">
          {#each visibleInboxMessages as message (message.message_id)}
            <article class="rounded-lg border p-3 text-sm">
              <div class="flex min-w-0 flex-wrap items-start justify-between gap-2">
                <p class="min-w-0 flex-1 whitespace-pre-wrap break-words font-medium">{message.input.summary}</p>
                <Badge variant={inboxBadgeVariant(message)}>{message.state}</Badge>
              </div>
              <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                <span>{message.delivery_policy}</span>
                <span>turn {shortId(message.turn_id)}</span>
                <span>{formatDateTime(message.updated_at)}</span>
              </div>
              {#if message.failure_message}
                <p class="mt-2 text-xs text-destructive">{message.failure_message}</p>
              {/if}
              {#if message.state === 'pending' || message.state === 'failed'}
                <div class="mt-3 flex flex-wrap justify-end gap-2">
                  {#if message.state === 'pending'}
                    <Button variant="outline" size="sm" class="gap-1.5" disabled={busyMessageId === message.message_id} aria-label={`Cancel inbox message ${message.input.summary}`} onclick={() => onCancel(message)}>
                      <X class="size-3.5" /> Cancel
                    </Button>
                  {:else if message.state === 'failed'}
                    <Button variant="outline" size="sm" class="gap-1.5" disabled={busyMessageId === message.message_id} aria-label={`Remove inbox message ${message.input.summary}`} onclick={() => onDismiss(message)}>
                      <X class="size-3.5" /> Remove
                    </Button>
                    <Button variant="outline" size="sm" class="gap-1.5" disabled={busyMessageId === message.message_id} aria-label={`Retry inbox message ${message.input.summary}`} onclick={() => onRetry(message)}>
                      <RotateCcw class="size-3.5" /> Retry
                    </Button>
                  {/if}
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {:else}
        <Empty.Root class="py-12">
          <Empty.Header>
            <Empty.Title>No inbox messages</Empty.Title>
            <Empty.Description>Follow-up messages submitted from this chat will appear here.</Empty.Description>
          </Empty.Header>
        </Empty.Root>
      {/if}
    </div>
  </Sheet.Content>
</Sheet.Root>
