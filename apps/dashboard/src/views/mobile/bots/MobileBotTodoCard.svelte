<script lang="ts">
  import type { BotMessage } from '../../../api/types'
  import { entryQuote, quoteSummary } from '$lib/botQuote'
  import { t } from '$lib/i18n'
  import MessageTime from '$lib/components/MessageTime.svelte'
  let {
    botName,
    item,
    onjump,
    oncontext,
    onreply,
    onquote,
    onchange,
  }: {
    botName: string
    item: BotMessage
    onjump: () => void
    oncontext: () => void
    onreply: () => void
    onquote: () => void
    onchange: (action: 'resolve' | 'reopen') => void
  } = $props()
  const done = $derived(item.todo?.state === 'done' ? item.todo : null)
</script>

<article class="my-2 rounded-xl border bg-card p-3">
  <button class="block min-h-11 w-full text-left" onclick={onjump}>
    <span class="text-xs text-muted-foreground"
      ><MessageTime value={item.created_at} /> · {done
        ? done.by === 'user'
          ? $t('bots.resolvedByYou')
          : $t('bots.closedByBot', { name: botName })
        : $t('bots.todos')}</span
    >
    <span class="line-clamp-3 break-words">{quoteSummary(entryQuote(item)!)}</span>
  </button>
  <div class="flex flex-wrap gap-1 text-sm [&>button]:min-h-11 [&>button]:min-w-11">
    <button class="mr-auto" onclick={oncontext}>{$t('bots.context')}</button>
    {#if !done}
      <button onclick={() => onchange('resolve')}>{$t('bots.ignore')}</button>
      <button onclick={onquote}>{$t('bots.quote')}</button>
      <button class="text-primary" onclick={onreply}>{$t('bots.reply')}</button>
    {:else if done.by === 'user'}<button onclick={() => onchange('reopen')}>{$t('bots.reopen')}</button>{/if}
  </div>
</article>
