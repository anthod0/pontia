<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { entryText, plainText, parseQuotedMessage } from '$lib/botQuote'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { useDeviceRuntime } from '../../../device/context'
  import type { Bot } from '../../../api/types'
  import MobileBotSheet from './MobileBotSheet.svelte'
  let {
    open = $bindable(false),
    bot,
    id,
    onjump,
  }: { open: boolean; bot: Bot; id: string; onjump: (id: string) => void } = $props()
  const device = useDeviceRuntime()
  const result = createQuery(
    () => ({
      queryKey: ['bot-message-context', bot.bot_id, id],
      queryFn: ({ signal }) => device.api.getBotMessageContext(bot.bot_id, id, signal),
      enabled: open,
    }),
    () => device.queryClient,
  )
</script>

<MobileBotSheet bind:open title={$t('bots.context')}>
  {#if result.error}<p role="alert">
      {result.error.message} <button onclick={() => result.refetch()}>{$t('common.retry')}</button>
    </p>
  {:else if result.data}
    {#if result.data.truncated}<p>{$t('bots.contextTruncated')}</p>{/if}
    {#each result.data.messages as message (message.id)}
      <button
        aria-current={message.id === id}
        class="mb-2 block min-h-11 w-full rounded-xl border p-3 text-left aria-current:border-primary"
        onclick={() => onjump(message.id)}
      >
        <strong>{message.kind === 'user_input' ? $t('bots.you') : bot.name}</strong>
        <MessageTime value={message.created_at} />
        {#if message.todo}<small>{$t(message.todo.state==='open'?'bots.todos':'bots.todoDone')}</small>{/if}
        {#if message.kind==='user_input' && parseQuotedMessage(message.content).quote}<small>{$t('bots.quoted')}</small>{/if}
        {#if message.status==='failed'}<small class="text-destructive">{$t('bots.sendFailed')}</small>{/if}
        <span class="block whitespace-pre-wrap break-words">{plainText(entryText(message))}</span>
      </button>
    {/each}
    {#if result.data.messages.length === 1}<p>{$t('bots.noRelated')}</p>{/if}
  {:else}<p role="status">{$t('common.loading')}</p>{/if}
</MobileBotSheet>
