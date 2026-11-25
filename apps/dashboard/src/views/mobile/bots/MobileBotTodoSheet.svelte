<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { entryQuote } from '$lib/botQuote'
  import type { BotChatController } from '../../../features/bots/botChat.svelte'
  import type { BotMessage } from '../../../api/types'
  import { useDeviceRuntime } from '../../../device/context'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import TodoViews from '../../../components/bots/TodoViews.svelte'
  import MobileBotSheet from './MobileBotSheet.svelte'
  import MobileBotTodoCard from './MobileBotTodoCard.svelte'
  import MobileBotReplySheet from './MobileBotReplySheet.svelte'
  let {
    chat,
    open = $bindable(false),
    oncontext,
    onjump,
  }: {
    chat: BotChatController
    open: boolean
    oncontext: (id: string) => void
    onjump: (id: string) => void
  } = $props()
  const device = useDeviceRuntime()
  const stack = useMobileStack()!
  const todos = createQuery(
    () => ({
      queryKey: ['bot-todos', chat.botId],
      queryFn: ({ signal }) => device.api.getBotTodos(chat.botId, signal),
      enabled: open,
      staleTime: 0,
    }),
    () => device.queryClient,
  )
  let reply = $state<BotMessage>()
  let replying = $state(false)
</script>

{#snippet card(item: BotMessage)}
  <MobileBotTodoCard
    {item}
    onjump={() => onjump(item.id)}
    oncontext={() => oncontext(item.id)}
    onreply={() => {
      reply = item
      replying = true
    }}
    onquote={() => stack.back(() => chat.addQuote(entryQuote(item)!))}
    onignore={() => chat.changeTodo(item.id, 'resolve')}
  />
{/snippet}
<MobileBotSheet bind:open title={$t('bots.todos')}>
  {#if todos.error}<p role="alert">
      {todos.error.message} <button onclick={() => todos.refetch()}>{$t('common.retry')}</button>
    </p>
  {:else if todos.data && chat.timeline.bot}
    {@const data = todos.data}
    <TodoViews count={data.open.length} groups={[{ bot: chat.timeline.bot, items: data.recently_resolved }]}
      onjump={(_, item) => onjump(item.id)} oncontext={(_, item) => oncontext(item.id)}
      onreopen={(_, item) => chat.changeTodo(item.id, 'reopen')}>
      {#snippet pending()}
        {#each data.open as item (item.id)}{@render card(item)}{:else}<p>{$t('bots.noTodos')}</p>{/each}
      {/snippet}
    </TodoViews>
  {:else}<p role="status">{$t('common.loading')}</p>{/if}
</MobileBotSheet>
{#if replying && reply}<MobileBotReplySheet
    bind:open={replying}
    message={reply}
    onsubmit={chat.post}
  />{/if}
