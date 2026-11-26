<script lang="ts">
  import type { Snippet } from 'svelte'
  import BotAvatar from './BotAvatar.svelte'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import ArrowUUpLeftIcon from 'phosphor-svelte/lib/ArrowUUpLeftIcon'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import { Button } from '$lib/components/ui/button'
  import * as Tabs from '$lib/components/ui/tabs'
  import { currentLocale, t } from '$lib/i18n'
  import { todoSummary } from '$lib/botQuote'
  import type { Bot, BotMessage } from '../../api/types'

  let { count, groups, pending, heading, status, showBot = false, onjump, oncontext, onreopen }: {
    count: number
    groups: { bot: Bot; items: BotMessage[] }[]
    heading?: Snippet
    pending: Snippet
    status?: Snippet
    showBot?: boolean
    onjump: (bot: Bot, item: BotMessage) => void
    oncontext: (bot: Bot, item: BotMessage) => void
    onreopen: (bot: Bot, item: BotMessage) => void
  } = $props()

  const rows = $derived(groups.flatMap(({ bot, items }) => items.flatMap(item =>
    item.todo?.state === 'done' ? [{ bot, item, done: item.todo, date: new Date(item.todo.at) }] : []
  )).sort((a, b) => b.date.getTime() - a.date.getTime()))
  const days = $derived.by(() => {
    const days = new Map<string, typeof rows>()
    for (const row of rows) {
      const day = row.date.toDateString()
      const group = days.get(day)
      if (group) group.push(row)
      else days.set(day, [row])
    }
    return days
  })
  function dayLabel(date: Date, locale: string) {
    const today = new Date()
    if (date.toDateString() === today.toDateString()) return $t('bots.today')
    const yesterday = new Date(today)
    yesterday.setDate(today.getDate() - 1)
    if (date.toDateString() === yesterday.toDateString()) return $t('bots.yesterday')
    return date.toLocaleDateString(locale, { year: 'numeric', month: 'short', day: 'numeric' })
  }
  const segment = $derived(['h-full min-h-0! flex-none rounded-chip px-2.5 text-xs aria-selected:bg-card group-data-[variant=default]/tabs-list:aria-selected:shadow-sm',
    !heading && 'dark:aria-selected:bg-[#45403d]',
  ])
</script>

{#snippet segments()}
  <Tabs.List aria-label={$t('bots.todos')} class="self-start rounded-action p-0.5 h-[30px] pointer-coarse:h-9">
    <Tabs.Trigger value="pending" class={segment}>{$t('bots.pending')} <span class="text-muted-foreground tabular-nums">{count}</span></Tabs.Trigger>
    <Tabs.Trigger value="resolved" class={segment}>{$t('bots.resolved')} <span class="text-muted-foreground tabular-nums">{rows.length}</span></Tabs.Trigger>
  </Tabs.List>
{/snippet}

<Tabs.Root value="pending" class="min-w-0 gap-3">
  {#if heading}
    <div class="sticky top-0 z-10 flex flex-wrap items-center gap-3 mr-11 bg-popover py-2">
      {@render heading()}
      {@render segments()}
    </div>
  {:else}
    {@render segments()}
  {/if}
  <Tabs.Content value="pending">{@render pending()}</Tabs.Content>
  <Tabs.Content value="resolved">
    {@render status?.()}
    {#each days as [day, entries] (day)}
      <section class="mb-4">
        <h3 class="px-1 py-1.5 text-xs font-medium text-muted-foreground">{dayLabel(entries[0].date, $currentLocale)}</h3>
        <ul class="divide-y divide-border max-md:rounded-item max-md:border max-md:bg-card max-md:px-2">
          {#each entries as {bot, item, done, date} (bot.bot_id + ':' + item.id)}
            <li class="flex items-center gap-1 px-1 py-1">
              <span aria-hidden="true" class="mr-1.5 flex size-4 shrink-0 items-center text-muted-foreground">
                {#if item.kind === 'user_input'}<CheckCircleIcon class="size-4" />
                {:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-4" />{/if}
              </span>
              <button type="button" class="min-w-0 flex-1 rounded-item py-1 text-left hover:text-primary focus-visible:outline-ring" onclick={() => onjump(bot, item)}>
                <span class="line-clamp-2 text-sm leading-5 [overflow-wrap:anywhere]">{todoSummary(item)}</span>
                <span class="text-xs text-muted-foreground">
                  {item.kind === 'user_input' ? $t('bots.you') : bot.name} · {#if showBot && item.kind === 'user_input' && done.by === 'user'}{bot.name} · {/if}
                  {done.by === 'user' ? $t('bots.resolvedByYou') : $t('bots.closedByBot', { name: bot.name })} ·
                  <time datetime={done.at} class="tabular-nums">{date.toLocaleTimeString($currentLocale, { hour: '2-digit', minute: '2-digit' })}</time>
                </span>
              </button>
              <Button variant="ghost" size="icon-xs" class="text-muted-foreground touch-target:size-11" aria-label={$t('bots.context')} title={$t('bots.context')} onclick={() => oncontext(bot, item)}><TreeStructureIcon /></Button>
                <Button variant="ghost" size="xs" class="text-muted-foreground touch-target:min-h-11" onclick={() => onreopen(bot, item)}><ArrowUUpLeftIcon />{$t('bots.reopen')}</Button>
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      {#if !status}<p role="status" class="px-1 py-6 text-sm text-muted-foreground">{$t('bots.noResolved')}</p>{/if}
    {/each}
  </Tabs.Content>
</Tabs.Root>
