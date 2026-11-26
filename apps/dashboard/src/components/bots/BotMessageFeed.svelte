<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte'
  import { get } from 'svelte/store'
  import { createVirtualizer } from '@tanstack/svelte-virtual'
  import { Button } from '$lib/components/ui/button/index.js'
  import ScrollToBottomButton from '../chat/ScrollToBottomButton.svelte'
  import { t } from '$lib/i18n'
  import { compareBotEntries, type BotTimeline } from '$lib/botTimeline.svelte'
  import type { BotEntry } from '../../api/types'

  let { timeline, typing, bottomInset=0, highlighted = null, root = $bindable(), row }: {
    timeline: BotTimeline
    root?: HTMLElement
    bottomInset?:number
    typing: boolean
    highlighted?: string | null
    row: Snippet<[BotEntry]>
  } = $props()
  let feed = $state<HTMLDivElement>()
  let bottom = $state<HTMLDivElement>()
  $effect(() => { root = feed })
  const entries = $derived.by(() => {
    const notice = timeline.generationNotice
    if (!notice) return timeline.entries
    const rows = timeline.entries.slice()
    const i = rows.findIndex(item => compareBotEntries(item, notice) > 0)
    rows.splice(i === -1 ? rows.length : i, 0, notice)
    return rows
  })
  let headerHeight = $state(0)
  let topInset = $state(0)
  const count = $derived(entries.length + Number(typing))
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => feed ?? null,
    estimateSize: () => 72,
    overscan: 5,
    gap: 20,
    paddingStart: 20,
    paddingEnd: 20,
    anchorTo: 'end',
    followOnAppend: true,
    scrollEndThreshold: 64,
  })
  let pinned = true
  let feedHeight = $state(0)
  const totalSize = $derived($virtualizer.getTotalSize())
  let intent = 0
  let firstId: string | undefined
  let initial = true
  let lastTop = 0
  const items = $derived($virtualizer.getVirtualItems())

  $effect.pre(() => {
    const element = feed
    const rows = entries
    const paddingStart = 20 + (timeline.before ? headerHeight + 12 : 0)
    const paddingEnd = 20 + bottomInset
    const length = count
    if (!element) return
    const previousFirst = firstId
    firstId = rows[0]?.id
    // Dynamic rows above the old first page are measured after Svelte commits
    // the virtualizer's estimated prepend. Keep the reader's actual pixel offset.
    if (previousFirst && firstId !== previousFirst && rows.some(row => row.id === previousFirst)) {
      const top = element.getBoundingClientRect().top
      const anchor = [...element.querySelectorAll<HTMLElement>('[data-bot-entry-id]')]
        .find(node => node.getBoundingClientRect().bottom > top)
      if (anchor && !pinned) {
        void restoreAnchor(anchor.dataset.botEntryId!, anchor.getBoundingClientRect().top - top, ++intent)
      }
    }
    untrack(() => get(virtualizer).setOptions({
      count: length,
      paddingStart,
      paddingEnd,
      scrollMargin: topInset,
      getItemKey: index => rows[index]?.id ?? 'typing',
    }))
    if (length && initial) {
      initial = false
      void tick().then(() => get(virtualizer).scrollToEnd())
    }
  })

  $effect(() => {
    const range = $virtualizer.range
    if (!range) return
    const visible = entries.slice(range.startIndex, range.endIndex + 1)
    const middle = (visible.length - 1) / 2
    const ids = visible.map((entry, index) => ({ entry, distance: Math.abs(index - middle) }))
      .sort((a, b) => a.distance - b.distance)
      .filter(({ entry }) => entry.id !== timeline.generationNotice?.id)
      .map(({ entry }) => entry.id)
    untrack(() => timeline.setVisible(ids))
  })

  // A decoded image or a wrapped code block can grow after the original append.
  // Pin against the committed sizer once Svelte has updated its DOM height.
  $effect(() => {
    if (totalSize && feedHeight && pinned) {
      void tick().then(() => { if (feed?.isConnected && pinned) get(virtualizer).scrollToEnd() })
    }
  })

  async function restoreAnchor(id: string, offset: number, version: number) {
    await tick()
    let settled = 0
    while (settled < 2 && feed?.isConnected && intent === version) {
      const node = feed.querySelector<HTMLElement>(`[data-bot-entry-id="${CSS.escape(id)}"]`)
      if (node) {
        const delta = node.getBoundingClientRect().top - feed.getBoundingClientRect().top - offset
        if (Math.abs(delta) <= 1) settled++
        else {
          settled = 0
          const before = feed.scrollTop
          feed.scrollTop += delta
          if (before === feed.scrollTop) return
        }
      } else {
        const index = entries.findIndex(row => row.id === id)
        if (index === -1) return
        get(virtualizer).scrollToIndex(index, { align: 'start' })
      }
      await new Promise<void>(resolve => requestAnimationFrame(() => resolve()))
    }
  }

  export function followSend() {
    intent++
    pinned = true
    get(virtualizer).scrollToEnd()
  }

  /** Scrolls a loaded entry to the middle of the feed. */
  export function scrollToEntry(id: string) {
    const index = entries.findIndex(row => row.id === id)
    intent++
    pinned = false
    get(virtualizer).scrollToIndex(index, { align: 'center' })
  }

  function measure(node: HTMLDivElement) {
    get(virtualizer).measureElement(node)
    return { destroy: () => get(virtualizer).measureElement(null) }
  }

  function scroll() {
    if (!feed) return
    const top = feed.scrollTop
    const distance = feed.scrollHeight - feed.clientHeight - top
    if (distance <= 64) pinned = true
    else if (top < lastTop) pinned = false
    if (top > lastTop && distance <= 1) {
      get(virtualizer).scrollToEnd()
    }
    if (top < lastTop && top < 64 && !timeline.historyError) timeline.loadOlder()
    lastTop = top
  }
</script>

<div class="relative flex min-h-0 flex-1 flex-col">
  <div bind:this={feed} bind:clientHeight={feedHeight} role="log" aria-label={$t('bots.messages')}
    onwheelcapture={() => intent++} ontouchstartcapture={() => intent++}
    onpointerdowncapture={() => intent++} onkeydowncapture={() => intent++} onscrollcapture={scroll}
    class="no-scrollbar min-h-0 flex-1 overflow-y-auto px-1 [overflow-anchor:none]">
    <!-- Keeps the oldest row clear of the transparent top bar the feed scrolls beneath. -->
    <div aria-hidden="true" class="h-14 md:h-12" bind:clientHeight={topInset}></div>
    <div class="relative w-full" style:height={`${totalSize}px`}>
      {#if timeline.before}
        <div bind:clientHeight={headerHeight} class="absolute top-1 left-0 flex w-full items-center justify-center gap-2 text-sm">
          {#if timeline.historyError}<span role="alert">{timeline.historyError}</span>{/if}
          <Button variant="ghost" size="sm" disabled={timeline.loadingOlder} onclick={() => timeline.loadOlder()}>
            {timeline.loadingOlder ? $t('chat.loadingEarlier') : timeline.historyError ? $t('common.retry') : $t('common.loadMore')}
          </Button>
        </div>
      {/if}
      {#each items as item (item.key)}
        <div class="group/entry absolute top-0 left-0 w-full" style:transform={`translateY(${item.start - topInset}px)`}
          data-index={item.index} data-bot-entry-id={entries[item.index]?.id}
          data-highlighted={highlighted !== null && entries[item.index]?.id === highlighted ? '' : undefined} use:measure>
          {#if entries[item.index]}
            {@render row(entries[item.index])}
          {:else if typing}
            <div aria-hidden="true" class="w-fit rounded-panel bg-muted px-4 py-3 text-sm text-muted-foreground"><span class="animate-pulse">•••</span></div>
          {/if}
        </div>
      {/each}
      <!-- As tall as the follow threshold, so the button appears once new rows stop following. -->
      <div aria-hidden="true" class="pointer-events-none absolute bottom-0 h-16 w-px" bind:this={bottom}></div>
    </div>
  </div>
  <ScrollToBottomButton sentinel={bottom} root={feed} onclick={followSend} class="absolute inset-x-0 bottom-2 z-10 px-1 [.mobile-bot-chat_&]:bottom-[calc(var(--mobile-footer-h)+var(--mobile-scroll-button-gap))]" />
</div>
