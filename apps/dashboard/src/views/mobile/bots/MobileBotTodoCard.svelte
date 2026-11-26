<script lang="ts">
  import type { BotMessage } from '../../../api/types'
  import { entryQuote, quoteSummary } from '$lib/botQuote'
  import { t } from '$lib/i18n'
  import MessageTime from '$lib/components/MessageTime.svelte'
  let {
    item,
    botName,
    onjump,
    oncontext,
    onreply,
    onquote,
    onignore,
  }: {
    botName: string
    item: BotMessage
    onjump: () => void
    oncontext: () => void
    onreply: () => void
    onquote: () => void
    onignore: () => void
  } = $props()
</script>

<article class="my-2 rounded-item border bg-card p-3">
  <button class="block min-h-11 w-full text-left" onclick={onjump}>
    <span class="text-xs text-muted-foreground"
      ><MessageTime value={item.created_at} /> · {item.kind === 'user_input' ? $t('bots.you') : botName}</span
    >
    <span class="line-clamp-3 break-words">{quoteSummary(entryQuote(item)!)}</span>
  </button>
  <div class="flex flex-wrap gap-1 text-sm [&>button]:min-h-11 [&>button]:min-w-11">
    <button class="mr-auto" onclick={oncontext}>{$t('bots.context')}</button>
    <button onclick={onignore}>{$t('bots.cancelTodo')}</button>
    <button onclick={onquote}>{$t('bots.quote')}</button>
    <button class="text-primary" onclick={onreply}>{$t('bots.reply')}</button>
  </div>
</article>
