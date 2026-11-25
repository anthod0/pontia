<script lang="ts">
  import { untrack } from 'svelte'
  import TodoViews from './TodoViews.svelte'
  import BotAvatar from './BotAvatar.svelte'
  import MobileBotTodoCard from '../../views/mobile/bots/MobileBotTodoCard.svelte'
  import MobileBotContextSheet from '../../views/mobile/bots/MobileBotContextSheet.svelte'
  import MobileBotReplySheet from '../../views/mobile/bots/MobileBotReplySheet.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { entryQuote, quotedMessage } from '$lib/botQuote'
  import { createQuoteDraft } from '$lib/botQuoteDraft'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { useDeviceRuntime } from '../../device/context'
  import type { createBotDirectory } from '../../features/bots/botDirectory.svelte'
  import { createBotTodos, setBotTodo } from '../../features/bots/botTodos.svelte'
  import type { Bot, BotMessage } from '../../api/types'
  let {directory}: {directory: ReturnType<typeof createBotDirectory>} = $props()
  const device = useDeviceRuntime()
  const stack = useMobileStack()
  const todos = createBotTodos(device, untrack(() => directory))
  let active = $state<{ bot: Bot; item: BotMessage }>()
  let context = $state(false),
    reply = $state(false)
  function jump(bot: Bot, id: string) {
    const go = () => navigate('/bots/' + encodeURIComponent(bot.bot_id), { message: id })
    if (stack?.layers.length) stack.back(go)
    else void go()
  }
  const openGroups = $derived(todos.groups.filter(({result}) => !result.data || result.data.open.length))
  const count = $derived(todos.groups.reduce((sum, {result}) => sum + (result.data?.open.length ?? 0), 0))
  const groups = $derived(todos.groups.map(({ bot, result }) => ({ bot, items: result.data?.recently_resolved ?? [] })))
  const error = $derived(directory.error ?? todos.groups.find(({result}) => result.error)?.result.error)
  const loading = $derived(directory.pending || todos.groups.some(({result}) => result.isPending))

  function viewContext(bot: Bot, item: BotMessage) {
    active = { bot, item }
    context = true
  }
</script>

{#snippet card(bot: Bot, item: BotMessage)}
  <MobileBotTodoCard
    {item} botName={bot.name}
    onjump={() => jump(bot, item.id)}
    oncontext={() => viewContext(bot, item)}
    onreply={() => {
      active = { bot, item }
      reply = true
    }}
    onquote={() => {
      createQuoteDraft(device.id, bot.bot_id).current = entryQuote(item)
      void navigate('/bots/' + encodeURIComponent(bot.bot_id))
    }}
    onignore={() => setBotTodo(device, bot.bot_id, item.id, false)}
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
{#snippet status()}
  {#if error}<p role="alert">{error.message}</p>{:else if loading}<p role="status">{$t('common.loading')}</p>{/if}
{/snippet}
<TodoViews {count} {groups} showBot status={error || loading ? status : undefined}
  onjump={(bot, item) => jump(bot, item.id)} oncontext={viewContext}
  onreopen={(bot, item) => setBotTodo(device, bot.bot_id, item.id, true)}>
  {#snippet pending()}
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
    </div>
  {/snippet}
</TodoViews>
