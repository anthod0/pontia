<script lang="ts">
  import type { Bot, BotMessage } from '../../../api/types'
  import { todoSummary } from '$lib/botQuote'
  import { t } from '$lib/i18n'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  let {
    item,
    bot,
    onjump,
    oncontext,
    onreply,
    onquote,
    onignore,
  }: {
    bot: Bot
    item: BotMessage
    onjump: () => void
    oncontext: () => void
    onreply: () => void
    onquote: () => void
    onignore: () => void
  } = $props()
</script>

<li class="px-1 pt-2">
  <button class="block min-h-11 w-full text-left" onclick={onjump}>
    <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
      {#if item.kind === 'user_input'}<CheckCircleIcon size={16} />
      {:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-4" />{/if}
      {item.kind === 'user_input' ? $t('bots.you') : bot.name} · <MessageTime value={item.created_at} />
    </span>
    <span class="mt-1 line-clamp-2 text-sm leading-5 [overflow-wrap:anywhere]">{todoSummary(item)}</span>
  </button>
  <div class="flex items-center justify-end gap-1 text-sm text-muted-foreground [&>button]:min-h-11 [&>button]:min-w-11">
    <button class="grid place-items-center" aria-label={$t('bots.context')} onclick={oncontext}><TreeStructureIcon size={18} /></button>
    <button class="grid place-items-center" aria-label={$t('bots.quote')} onclick={onquote}><QuotesIcon size={18} /></button>
    <button class="px-1.5" onclick={onignore}>{$t('bots.cancelTodo')}</button>
    <button class="px-1.5 font-medium text-primary" onclick={onreply}>{$t('bots.reply')}</button>
  </div>
</li>
