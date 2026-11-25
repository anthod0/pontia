<script lang="ts">
  import { createInfiniteQuery } from '@tanstack/svelte-query'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import ImageIcon from 'phosphor-svelte/lib/ImageIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import MessageTime from '$lib/components/MessageTime.svelte'
  import { t } from '$lib/i18n'
  import { markMatches, searchSnippet } from '$lib/botSearch'
  import type { Bot, BotMessageAttachment, BotSearchHit, BotSearchType } from '../../api/types'
  import { useDeviceRuntime } from '../../device/context'
  import BotAvatar from './BotAvatar.svelte'
  import QuoteThumbnail from './QuoteThumbnail.svelte'

  let { bot, active, text = $bindable(''), filter = $bindable('all'), fileSessionId = null, selected, onselect }: {
    bot: Bot
    /** Whether this tab is showing; hidden tabs stay mounted. */
    active: boolean
    /** The typed search text, kept by the panel while this tab is hidden. */
    text?: string
    /** Whether to search message text, files or images, kept like `text`. */
    filter?: BotSearchType
    /** The Session that reads the Bot's shared files, for image previews. */
    fileSessionId?: string | null
    selected: string | null
    /** Jumps to a message; `path` names its shared file to highlight. */
    onselect: (id: string, path: string | null) => void
  } = $props()
  const device = useDeviceRuntime()
  // svelte-ignore state_referenced_locally
  let query = $state(text.trim())
  let timer: ReturnType<typeof setTimeout> | undefined
  let field = $state<HTMLInputElement | null>(null)
  $effect(() => () => clearTimeout(timer))
  $effect(() => { if (active) field?.focus() })

  // File and image searches list everything without search text.
  const searching = $derived(filter !== 'all' || query !== '')
  const results = createInfiniteQuery(() => ({
    queryKey: ['bot-message-search', bot.bot_id, filter, query],
    queryFn: ({ pageParam, signal }) => device.api.searchBotMessages(bot.bot_id, { q: query, type: filter, before: pageParam }, signal),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: page => page.next_before ?? undefined,
    enabled: active && searching,
    staleTime: 0,
  }), () => device.queryClient)
  const messages = $derived(searching ? results.data?.pages.flatMap(page => page.messages) ?? [] : [])
  const files = $derived(filter === 'all' ? [] : messages.flatMap(message => (message.attachments ?? []).map(file => ({ message, file }))))
  const filters = $derived([
    { value: 'all', label: $t('common.all') },
    { value: 'file', label: $t('bots.searchFiles') },
    { value: 'image', label: $t('bots.searchImages') },
  ] as const)
  const noResults = $derived(filter === 'file' ? $t('bots.searchNoFiles') : filter === 'image' ? $t('bots.searchNoImages') : $t('bots.searchNoResults'))

  function input(value: string) {
    text = value
    clearTimeout(timer)
    timer = setTimeout(() => { query = value.trim() }, 250)
  }

  function clear() {
    clearTimeout(timer)
    text = query = ''
  }

  // Only attachments blocks render as shared files that can carry a highlight.
  function selectFile(message: BotSearchHit, file: BotMessageAttachment) {
    onselect(message.id, file.source === 'attachment_block' && fileSessionId ? file.path : null)
  }
</script>

{#snippet marked(value: string)}{#each markMatches(value, query) as part}{#if part.match}<mark class="rounded-sm bg-primary/15 px-0.5 text-foreground">{part.text}</mark>{:else}{part.text}{/if}{/each}{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  <div class="shrink-0 space-y-2 border-b p-3">
    <div class="relative">
      <MagnifyingGlassIcon class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input type="search" class="pr-8 pl-8 [&::-webkit-search-cancel-button]:hidden" placeholder={$t('bots.searchPlaceholder')} aria-label={$t('bots.search')}
        value={text} oninput={event => input(event.currentTarget.value)} maxlength={200}
        onkeydown={event => { if (event.key === 'Escape') clear() }} bind:ref={field} />
      {#if text}
        <button type="button" class="absolute top-1/2 right-1.5 inline-flex size-[18px] shrink-0 -translate-y-1/2 items-center justify-center rounded-full p-0 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label={$t('common.clear')} onclick={clear}><XIcon class="size-3.5" /></button>
      {/if}
    </div>
    <ToggleGroup.Root type="single" variant="outline" size="sm" class="w-full" aria-label={$t('bots.searchType')}
      value={filter} onValueChange={value => { if (value) filter = value as BotSearchType }}>
      {#each filters as option (option.value)}
        <ToggleGroup.Item value={option.value} class="flex-1">
          {#if option.value === 'file'}<FileIcon />{:else if option.value === 'image'}<ImageIcon />{/if}
          {option.label}
        </ToggleGroup.Item>
      {/each}
    </ToggleGroup.Root>
  </div>
  <div class="min-h-0 flex-1 overflow-auto overscroll-contain p-3">
    {#if !searching}
      <p class="px-2 py-6 text-center text-sm text-muted-foreground">{$t('bots.searchHint')}</p>
    {:else if results.isError && !messages.length}
      <div role="alert" class="text-sm">{results.error.message} <Button variant="ghost" size="xs" onclick={() => results.refetch()}>{$t('common.retry')}</Button></div>
    {:else if results.isPending}
      <p role="status" class="text-sm text-muted-foreground">{$t('common.loading')}</p>
    {:else if !messages.length}
      <Empty.Root class="min-h-0 p-6"><Empty.Header>
        <Empty.Media class="size-12 rounded-xl bg-muted text-muted-foreground">
          {#if filter === 'file'}<FileIcon class="size-6" />{:else if filter === 'image'}<ImageIcon class="size-6" />{:else}<MagnifyingGlassIcon class="size-6" />{/if}
        </Empty.Media>
        <Empty.Title>{noResults}</Empty.Title>
      </Empty.Header></Empty.Root>
    {:else if filter === 'all'}
      <ul class="space-y-1" aria-label={$t('bots.searchResults')} aria-busy={results.isFetching}>
        {#each messages as message (message.id)}
          {@const user = message.kind === 'user_input'}
          <li>
            <button type="button" aria-current={message.id === selected ? 'true' : undefined}
              class={['flex w-full gap-2.5 rounded-xl p-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring', message.id === selected && 'bg-muted']}
              onclick={() => onselect(message.id, null)}>
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
    {:else}
      <ul class="space-y-1" aria-label={$t('bots.searchResults')} aria-busy={results.isFetching}>
        {#each files as { message, file } (`${message.id}\n${file.path}`)}
          {@const user = message.kind === 'user_input'}
          <li>
            <button type="button" aria-current={message.id === selected ? 'true' : undefined} title={file.path}
              class={['flex w-full items-center gap-2.5 rounded-xl p-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring', message.id === selected && 'bg-muted']}
              onclick={() => selectFile(message, file)} data-search-file={file.path}>
              <span class="grid size-10 shrink-0 place-items-center overflow-hidden rounded-lg border bg-muted text-muted-foreground">
                <!-- Uploads are temporary files the Bot's file Session may not read. -->
                {#if file.media_kind === 'image' && !user && fileSessionId}<QuoteThumbnail sessionId={fileSessionId} path={file.path} class="size-full" />
                {:else if file.media_kind === 'image'}<ImageIcon class="size-5" />
                {:else}<FileIcon class="size-5" />{/if}
              </span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-[13px] leading-5 font-medium text-heading">{@render marked(file.name)}</span>
                <span class="block truncate text-xs text-muted-foreground">{@render marked(file.path)}</span>
                <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span class="truncate">{user ? $t('bots.you') : bot.name}</span>
                  <MessageTime value={message.created_at} class="ml-auto shrink-0 px-0" />
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if searching && messages.length && results.hasNextPage}
      <div class="flex items-center justify-center gap-2 pt-2 text-sm">
        {#if results.isFetchNextPageError}<span role="alert">{results.error?.message}</span>{/if}
        <Button variant="outline" size="sm" disabled={results.isFetchingNextPage} onclick={() => results.fetchNextPage()}>{results.isFetchingNextPage ? $t('common.loading') : results.isFetchNextPageError ? $t('common.retry') : $t('common.loadMore')}</Button>
      </div>
    {/if}
  </div>
</div>
