<script lang="ts">
  import BotAvatar from '../../components/bots/BotAvatar.svelte'
  import MobileBotTodoCard from './bots/MobileBotTodoCard.svelte'
  import MobileBotContextSheet from './bots/MobileBotContextSheet.svelte'
  import MobileBotReplySheet from './bots/MobileBotReplySheet.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { entryQuote, quotedMessage } from '$lib/botQuote'
  import { createQuoteDraft } from '$lib/botQuoteDraft'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { useDeviceRuntime } from '../../device/context'
  import { useMobileDirectory } from '../../features/bots/mobileDirectory'
  import { createBotTodos, changeBotTodo } from '../../features/bots/botTodos.svelte'
  import type { Bot, BotMessage } from '../../api/types'
  const directory = useMobileDirectory()!
  const device = useDeviceRuntime()
  const stack = useMobileStack()!
  const todos = createBotTodos(device, directory, true)
  let active = $state<{ bot: Bot; item: BotMessage }>()
  let context = $state(false),
    reply = $state(false)
  function jump(bot: Bot, id: string) {
    const go = () => navigate('/bots/' + encodeURIComponent(bot.bot_id), { message: id })
    if (stack.layers.length) stack.back(go)
    else void go()
  }
  const openGroups = $derived(todos.groups.filter(({result}) => !result.data || result.data.open.length))
  const resolved = $derived(
    todos.groups.flatMap(({ bot, result }) => (result.data?.recently_resolved ?? []).map((item) => ({ bot, item }))),
  )
</script>

{#snippet card(bot: Bot, item: BotMessage)}
  <MobileBotTodoCard
    botName={bot.name}
    {item}
    onjump={() => jump(bot, item.id)}
    oncontext={() => {
      active = { bot, item }
      context = true
    }}
    onreply={() => {
      active = { bot, item }
      reply = true
    }}
    onquote={() => {
      createQuoteDraft(device.id, bot.bot_id).current = entryQuote(item)
      void navigate('/bots/' + encodeURIComponent(bot.bot_id))
    }}
    onchange={(action) => changeBotTodo(device, bot.bot_id, item.id, action)}
  />
{/snippet}
{#if active && context}<MobileBotContextSheet
    bind:open={context}
    bot={active.bot}
    id={active.item.id}
    onjump={(id) => jump(active!.bot, id)}
  />{/if}
{#if active && reply}
  {@const bot = active.bot}
  <MobileBotReplySheet
    bind:open={reply}
    message={active.item}
    onsubmit={async (text, quote) => {
      const sent = await device.api.sendBotMessage(bot.bot_id, quotedMessage(quote, text))
      toast.success($t('bots.replySent'), {
        action: { label: $t('bots.viewReply'), onClick: () => jump(bot, sent.id) },
      })
    }}
  />
{/if}
<div class="grid gap-5">
  {#each openGroups as { bot, result } (bot.bot_id)}
    <section class="grid gap-2">
      <h2>
        <button
          class="flex min-h-11 items-center gap-2 font-semibold"
          onclick={() => navigate('/bots/' + encodeURIComponent(bot.bot_id))}
        >
          <BotAvatar class="size-7" shape={bot.avatar_shape} color={bot.avatar_color} />{bot.name}
        </button>
      </h2>
      {#if result.error}<p role="alert">{result.error.message}</p>{:else if result.isPending}<p role="status">
          {$t('common.loading')}
        </p>{/if}
      {#each result.data?.open ?? [] as item (item.id)}{@render card(bot, item)}{/each}
    </section>
  {:else}
    <p role="status" class="py-6 text-muted-foreground">
      {directory.error?.message ?? $t(directory.pending ? 'common.loading' : 'bots.noTodos')}
    </p>
  {/each}
  {#if resolved.length}
    <details>
      <summary class="cursor-pointer py-3 text-sm text-muted-foreground">{$t('bots.recentlyResolved')}</summary>
      {#each resolved as { bot, item } (bot.bot_id + ':' + item.id)}
        <p class="mt-2 text-sm font-semibold">{bot.name}</p>
        {@render card(bot, item)}
      {/each}
    </details>
  {/if}
</div>
