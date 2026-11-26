<script lang="ts">
  import { t, currentLocale } from '$lib/i18n'
  import { formatDateTime, formatRelativeTime } from '$lib/format'
  import { useDeviceRuntime } from '../../device/context'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import BrainIcon from 'phosphor-svelte/lib/BrainIcon'
  import MagnifyingGlassIcon from 'phosphor-svelte/lib/MagnifyingGlassIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { memoryKindLabel } from './kinds'

  const device = useDeviceRuntime()
  const workspaces = device.workspaceQueries.createWorkspacesQuery()

  let search = $state('')
  let query = $state('')
  let timer: ReturnType<typeof setTimeout> | undefined

  const memories = device.memoryQueries.createMemoriesQuery(() => query)
  const items = $derived(memories.data?.pages.flatMap((page) => page.memories) ?? [])
  const total = $derived(memories.data?.pages[0]?.total ?? 0)

  function onSearch(value: string): void {
    search = value
    clearTimeout(timer)
    timer = setTimeout(() => query = value.trim(), 250)
  }

  function clearSearch(): void {
    clearTimeout(timer)
    search = ''
    query = ''
  }

  function workspaceName(id: string | null): string | null {
    if (!id) return null
    const found = workspaces.data?.find((item) => item.workspace_id === id)
    return found?.name ?? found?.display_path ?? id
  }

  function highlight(text: string): { text: string, match: boolean }[] {
    if (!query) return [{ text, match: false }]
    const lower = text.toLowerCase(), needle = query.toLowerCase()
    const parts: { text: string, match: boolean }[] = []
    let index = 0
    while (index < text.length) {
      const found = lower.indexOf(needle, index)
      if (found < 0) { parts.push({ text: text.slice(index), match: false }); break }
      if (found > index) parts.push({ text: text.slice(index, found), match: false })
      parts.push({ text: text.slice(found, found + needle.length), match: true })
      index = found + needle.length
    }
    return parts
  }
</script>

{#if memories.isError}
  <Alert.Root variant="destructive"><WarningCircleIcon class="size-4" /><Alert.Description>{memories.error.message}</Alert.Description></Alert.Root>
{:else if memories.isPending}
  <div class="space-y-3" role="status" aria-label={$t('common.loading')}><Skeleton class="h-9 w-full" /><Skeleton class="h-12 w-full" /><Skeleton class="h-12 w-full" /></div>
{:else if !items.length && !query}
  <Empty.Root class="min-h-72">
    <Empty.Header>
      <Empty.Media variant="icon"><BrainIcon /></Empty.Media>
      <Empty.Title>{$t('memory.emptyTitle')}</Empty.Title>
      <Empty.Description>{$t('memory.emptyDescription')}</Empty.Description>
    </Empty.Header>
  </Empty.Root>
{:else}
  <div class="space-y-2">
    <div class="flex items-center gap-3">
      <div class="relative flex-1">
        <MagnifyingGlassIcon class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input class="pr-8 pl-8" placeholder={$t('memory.searchPlaceholder')} aria-label={$t('memory.search')} value={search} oninput={(event) => onSearch(event.currentTarget.value)} />
        {#if search}
          <button type="button" class="absolute top-1/2 right-1.5 -translate-y-1/2 rounded-tooltip p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground" aria-label={$t('common.clear')} onclick={clearSearch}><XIcon class="size-3.5" /></button>
        {/if}
      </div>
      <p class="shrink-0 text-sm text-muted-foreground" aria-live="polite">{query ? $t('memory.resultCount', { count: total }) : $t('memory.count', { count: total })}</p>
    </div>

    {#if !items.length}
      <Empty.Root class="min-h-64">
        <Empty.Header>
          <Empty.Title>{$t('memory.noMatchTitle')}</Empty.Title>
          <Empty.Description>{$t('memory.noMatchDescription')}</Empty.Description>
        </Empty.Header>
        <Empty.Content><Button size="sm" variant="outline" onclick={clearSearch}>{$t('memory.clearSearch')}</Button></Empty.Content>
      </Empty.Root>
    {:else}
      <ul class="divide-y" aria-label={$t('memory.entries')}>
        {#each items as memory (memory.id)}
          {@const workspace = workspaceName(memory.workspace_id)}
          <li class="space-y-1 px-1 py-3 sm:px-2">
            <p class="whitespace-pre-wrap">{#each highlight(memory.content) as part}{#if part.match}<mark class="rounded-tooltip bg-primary/15 px-0.5 text-foreground">{part.text}</mark>{:else}{part.text}{/if}{/each}</p>
            <p class="truncate text-sm text-muted-foreground">
              {$t(memoryKindLabel(memory.kind))}{#if workspace}{' · '}{workspace}{/if}{' · '}<time datetime={memory.updated_at} title={formatDateTime(memory.updated_at, $currentLocale)}>{formatRelativeTime(memory.updated_at, $currentLocale)}</time>
            </p>
          </li>
        {/each}
      </ul>
      {#if memories.hasNextPage}
        <div class="flex justify-center pt-2">
          <Button variant="outline" disabled={memories.isFetchingNextPage} onclick={() => memories.fetchNextPage()}>{memories.isFetchingNextPage ? $t('common.loading') : $t('common.loadMore')}</Button>
        </div>
      {/if}
    {/if}
  </div>
{/if}
