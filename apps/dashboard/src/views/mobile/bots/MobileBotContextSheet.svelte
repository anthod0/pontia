<script lang="ts">
  import { tick } from 'svelte'
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../device/context'
  import type { Bot } from '../../../api/types'
  import BotContextList from '../../../components/bots/BotContextList.svelte'
  import MobileBotSheet from './MobileBotSheet.svelte'
  let {
    open = $bindable(false),
    bot,
    id,
    fileLinkSession = null,
    onjump,
  }: { open: boolean; bot: Bot; id: string; fileLinkSession?: { session_id: string; workspace: string | null } | null; onjump: (id: string, path?: string | null) => void } = $props()
  const device = useDeviceRuntime()
  const result = createQuery(
    () => ({
      queryKey: ['bot-message-context', bot.bot_id, id],
      queryFn: ({ signal }) => device.api.getBotMessageContext(bot.bot_id, id, signal),
      enabled: open,
    }),
    () => device.queryClient,
  )
  const messages = $derived(result.data?.messages)
  const range = $derived(messages?.length ? `${new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date(messages[0].created_at))} – ${new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date(messages.at(-1)!.created_at))}` : '')
  function jump(id: string, path: string | null = null) {
    open = false
    void tick().then(() => onjump(id, path))
  }
</script>

<MobileBotSheet bind:open fixed title={$t('bots.context')} subtitle={messages ? `${$t('bots.contextCount', { count: messages.length })}${range ? ` · ${range}` : ''}` : undefined}>
  <BotContextList
    {bot}
    selected={id}
    {messages}
    truncated={result.data?.truncated}
    error={result.error}
    mobile
    {fileLinkSession}
    onretry={() => void result.refetch()}
    onjump={jump}
  />
</MobileBotSheet>
