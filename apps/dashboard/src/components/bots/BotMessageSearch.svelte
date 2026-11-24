<script lang="ts">
  import { createInfiniteQuery } from '@tanstack/svelte-query'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { t } from '$lib/i18n'
  import { searchSnippet } from '$lib/botSearch'
  import type { Bot } from '../../api/types'
  import { useDeviceRuntime } from '../../device/context'
  import BotAvatar from './BotAvatar.svelte'

  let { bot, active, text = $bindable(''), selected, onselect }: {
    bot: Bot
    /** Whether this tab is showing; hidden tabs stay mounted. */
    active: boolean
    /** The typed search text, kept by the panel while this tab is hidden. */
    text?: string
    selected: string | null
    onselect: (id: string) => void
  } = $props()
  const device = useDeviceRuntime()
  // svelte-ignore state_referenced_locally
  let query = $state(text.trim())
  let timer: ReturnType<typeof setTimeout> | undefined
  let field = $state<HTMLInputElement | null>(null)
  $effect(() => () => clearTimeout(timer))
  $effect(() => { if (active) field?.focus() })

  const results = createInfiniteQuery(() => ({
    queryKey: ['bot-message-search', bot.bot_id, query],
    queryFn: ({ pageParam, signal }) => device.api.searchBotMessages(bot.bot_id, { q: query, before: pageParam }, signal),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: page => page.next_before ?? undefined,
    enabled: active && query !== '',
    staleTime: 0,
  }), () => device.queryClient)
  const messages = $derived(query ? results.data?.pages.flatMap(page => page.messages) ?? [] : [])

  function input(value: string) {
    text = value
    clearTimeout(timer)
    timer = setTimeout(() => { query = value.trim() }, 250)
  }

  function clear() {
    clearTimeout(timer)
    text = query = ''
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="shrink-0 border-b p-3">
    <div class="relative">
      <MagnifyingGlassIcon class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input type="search" class="pr-8 pl-8 [&::-webkit-search-cancel-button]:hidden" placeholder={$t('bots.searchPlaceholder')} aria-label={$t('bots.search')}
        value={text} oninput={event => input(event.currentTarget.value)} maxlength={200}
        onkeydown={event => { if (event.key === 'Escape') clear() }} bind:ref={field} />
      {#if text}
        <button type="button" class="absolute top-1/2 right-1.5 inline-flex size-[18px] shrink-0 -translate-y-1/2 items-center justify-center rounded-full p-0 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label={$t('common.clear')} onclick={clear}><XIcon class="size-3.5" /></button>
      {/if}
    </div>
  </div>
  <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-3">
    {#if !query}
      <p class="px-2 py-6 text-center text-sm text-muted-foreground">{$t('bots.searchHint')}</p>
    {:else if results.isError && !messages.length}
      <div role="alert" class="text-sm">{results.error.message} <Button variant="ghost" size="xs" onclick={() => results.refetch()}>{$t('common.retry')}</Button></div>
    {:else if results.isPending}
      <p role="status" class="text-sm text-muted-foreground">{$t('common.loading')}</p>
    {:else if !messages.length}
      <Empty.Root class="min-h-0 p-6"><Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground"><MagnifyingGlassIcon class="size-6" /></Empty.Media>
        <Empty.Title>{$t('bots.searchNoResults')}</Empty.Title>
      </Empty.Header></Empty.Root>
    {:else}
      <ul class="space-y-1" aria-label={$t('bots.searchResults')} aria-busy={results.isFetching}>
        {#each messages as message (message.id)}
          {@const user = message.kind === 'user_input'}
          <li>
            <button type="button" aria-current={message.id === selected ? 'true' : undefined}
              class={['flex w-full gap-2.5 rounded-xl p-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring', message.id === selected && 'bg-muted']}
              onclick={() => onselect(message.id)}>
              {#if user}<span aria-hidden="true" class="grid size-5 shrink-0 place-items-center rounded-full bg-border text-[10px] font-semibold text-heading">{$t('bots.you').slice(0, 1)}</span>
              {:else}<BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-5 shrink-0" />{/if}
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span class="truncate font-medium text-heading">{user ? $t('bots.you') : bot.name}</span>
                  <MessageTime value={message.created_at} class="ml-auto shrink-0 px-0" />
                </span>
                <span class="line-clamp-3 text-[13px] leading-5 [overflow-wrap:anywhere]">{#each searchSnippet(message, query) as part}{#if part.match}<mark class="rounded-sm bg-primary/15 px-0.5 text-foreground">{part.text}</mark>{:else}{part.text}{/if}{/each}</span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if results.hasNextPage}
        <div class="flex items-center justify-center gap-2 pt-2 text-sm">
          {#if results.isFetchNextPageError}<span role="alert">{results.error?.message}</span>{/if}
          <Button variant="outline" size="sm" disabled={results.isFetchingNextPage} onclick={() => results.fetchNextPage()}>{results.isFetchingNextPage ? $t('common.loading') : results.isFetchNextPageError ? $t('common.retry') : $t('common.loadMore')}</Button>
        </div>
      {/if}
    {/if}
  </div>
</div>
