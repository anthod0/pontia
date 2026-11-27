<script lang="ts">
  import { parseUploadMessage } from "$lib/uploads/message"
  import { t } from '$lib/i18n';
  import TrashIcon from 'phosphor-svelte/lib/TrashIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import type { QueuedMessageView } from '../../api/types'

  interface Props {
    sessionId: string
    messages: QueuedMessageView[]
    busyMessageId: string | null
    onCancel: (message: QueuedMessageView) => void
  }

  let { sessionId, messages, busyMessageId, onCancel }: Props = $props()
  let visibleMessages = $derived(messages.filter((message) => message.session_id === sessionId))
</script>

{#if visibleMessages.length}
  <section class="mb-2 overflow-hidden rounded-item border bg-background shadow-none" aria-label={$t('queue.title')}>
    <ul class="max-h-40 overflow-y-auto py-1">
      {#each visibleMessages as message (message.message_id)}
        {@const parts = parseUploadMessage(message.input)}
        {@const summary = parts.text || [...parts.files.map(file => file.name), ...parts.secrets.map(secret => secret.label || secret.masked)].join(', ')}
        <li class="group flex min-w-0 items-center gap-2 px-3 py-1.5 text-sm">
          <div class="min-w-0 flex-1">
            <p class="truncate" title={summary}>{summary}</p>
            <p class="text-xs text-muted-foreground">{$t('common.waiting')}</p>
          </div>
          <div class="flex shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
            <Button variant="ghost" size="icon-xs" disabled={busyMessageId === message.message_id} aria-label={$t('queue.cancelMessage', { summary: summary })} title={$t('common.cancel')} onclick={() => onCancel(message)}>
              <TrashIcon class="size-3.5" />
            </Button>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}
