<script lang="ts">
  import { tick } from 'svelte'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { copyText } from '$lib/copyText'
  import { entryQuote, messageText, quoteSummary, type BotQuote } from '$lib/botQuote'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import CursorTextIcon from 'phosphor-svelte/lib/CursorTextIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { downloads } from '$lib/downloads/manager'
  import { canPreviewImage } from '$lib/downloads/preview'
  import { useDeviceRuntime } from '../../device/context'
  import type { BotEntry, SharedFile } from '../../api/types'

  let { root, entry, selectableId = $bindable(null), onquote }: {
    /** Element containing the feed rows; events are delegated from it. */
    root: HTMLElement | undefined
    entry: (id: string) => BotEntry | undefined
    /** The row whose text a touch user chose to select. */
    selectableId?: string | null
    onquote: (quote: BotQuote) => void
  } = $props()

  /** A message, or one file of a share when `file` is set. */
  type Target = { entry: BotEntry; element: HTMLElement; excerpt: string | null; file: SharedFile | null }
  const LONG_PRESS_MS = 500
  const device = useDeviceRuntime()

  let target = $state<Target | null>(null)
  let menuOpen = $state(false)
  let sheetOpen = $state(false)
  let anchor = $state({ x: 0, y: 0 })
  let selection = $state<{ target: Target; rect: DOMRect; touch: boolean } | null>(null)
  let touchSelection = false
  let pointerDown = false
  let press: { timer: ReturnType<typeof setTimeout>; x: number; y: number } | null = null
  let suppressClick = false

  const menuAnchor = { getBoundingClientRect: () => DOMRect.fromRect({ x: anchor.x, y: anchor.y, width: 0, height: 0 }) }

  type Row = Omit<Target, 'excerpt'>
  // Files are quoted one at a time: each file of a share is its own quotable element.
  function rowOf(node: Node | null): Row | null {
    const element = (node instanceof Element ? node : node?.parentElement)?.closest<HTMLElement>('[data-bot-quotable]')
    const id = element?.closest<HTMLElement>('[data-bot-entry-id]')?.dataset.botEntryId
    const found = id ? entry(id) : undefined
    if (!element || !found || !root?.contains(element)) return null
    const fileId = element.dataset.botFileId
    const file = fileId && found.kind === 'shared_files' ? found.files.find(file => file.file_id === fileId) ?? null : null
    return { entry: found, element, file }
  }

  // A selection counts only when it lies within the text of one message.
  function selectedTarget(): Target | null {
    const current = document.getSelection()
    if (!current || current.isCollapsed || !current.rangeCount) return null
    const range = current.getRangeAt(0)
    const source = (range.commonAncestorContainer instanceof Element ? range.commonAncestorContainer : range.commonAncestorContainer.parentElement)?.closest('[data-bot-quote-source]')
    const row = source ? rowOf(source) : null
    const text = current.toString().trim()
    return row && text ? { ...row, excerpt: text } : null
  }

  // A shared image opens the file menu; other images and links keep the browser's.
  function keepsNativeMenu(event: MouseEvent): boolean {
    if (event.shiftKey || !(event.target instanceof Element)) return event.shiftKey
    const native = event.target.closest('a, img, video, input, textarea, [contenteditable="true"]')
    return Boolean(native && !(native.tagName === 'IMG' && native.closest('[data-bot-file-id]')))
  }

  function open(next: Target, touch: boolean, point: { x: number; y: number }) {
    selection = null
    target = next
    anchor = point
    if (touch) sheetOpen = true
    else menuOpen = true
  }

  function contextmenu(event: MouseEvent) {
    const row = rowOf(event.target as Node)
    if (!row || keepsNativeMenu(event)) return
    event.preventDefault()
    if ((event as PointerEvent).pointerType === 'touch' || press) {
      // Android fires contextmenu on long press; iOS relies on the timer below.
      if (press) longPress(row)
      return
    }
    const selected = selectedTarget()
    open(selected?.element === row.element ? selected : { ...row, excerpt: null }, false, { x: event.clientX, y: event.clientY })
  }

  function longPress(row: Row) {
    if (!press) return
    clearTimeout(press.timer)
    const point = { x: press.x, y: press.y }
    press = null
    suppressClick = true
    open({ ...row, excerpt: null }, true, point)
  }

  function pointerdown(event: PointerEvent) {
    pointerDown = true
    touchSelection = event.pointerType === 'touch'
    if (event.pointerType !== 'touch' || keepsNativeMenu(event)) return
    const row = rowOf(event.target as Node)
    if (!row || row.entry.id === selectableId) return
    press = { timer: setTimeout(() => longPress(row), LONG_PRESS_MS), x: event.clientX, y: event.clientY }
  }

  function pointermove(event: PointerEvent) {
    if (press && Math.hypot(event.clientX - press.x, event.clientY - press.y) > 10) cancelPress()
  }

  function cancelPress() {
    if (press) clearTimeout(press.timer)
    press = null
  }

  function pointerup() {
    pointerDown = false
    cancelPress()
    updateSelection()
  }

  // The click that ends a long press would otherwise land on the opened sheet.
  function click(event: MouseEvent) {
    if (!suppressClick) return
    suppressClick = false
    event.preventDefault()
    event.stopPropagation()
  }

  function updateSelection() {
    if (pointerDown && !touchSelection) return
    const selected = selectedTarget()
    if (!selected) {
      selection = null
      if (selectableId && !document.getSelection()?.toString()) selectableId = null
      return
    }
    selection = { target: selected, rect: document.getSelection()!.getRangeAt(0).getBoundingClientRect(), touch: touchSelection }
  }

  function keydown(event: KeyboardEvent) {
    if (!(event.target instanceof HTMLElement) || !event.target.matches('[data-bot-quotable]')) return
    const row = rowOf(event.target)
    if (!row) return
    if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      event.preventDefault()
      const rect = row.element.getBoundingClientRect()
      open({ ...row, excerpt: null }, false, { x: rect.left, y: rect.bottom })
    } else if (event.key.toLowerCase() === 'r' && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault()
      quote({ ...row, excerpt: null })
    } else if (event.key.toLowerCase() === 'c' && (event.ctrlKey || event.metaKey) && !document.getSelection()?.toString()) {
      event.preventDefault()
      void copy({ ...row, excerpt: null })
    }
  }

  $effect(() => {
    const element = root
    if (!element) return
    const listeners: [string, EventListener, boolean?][] = [
      ['contextmenu', contextmenu as EventListener],
      ['pointerdown', pointerdown as EventListener],
      ['pointermove', pointermove as EventListener],
      ['pointerup', pointerup],
      ['pointercancel', pointerup],
      ['click', click as EventListener, true],
      ['keydown', keydown as EventListener],
      ['scroll', updateSelection, true],
    ]
    for (const [type, listener, capture] of listeners) element.addEventListener(type, listener, capture)
    document.addEventListener('selectionchange', updateSelection)
    return () => {
      for (const [type, listener, capture] of listeners) element.removeEventListener(type, listener, capture)
      document.removeEventListener('selectionchange', updateSelection)
      cancelPress()
    }
  })

  function quote(next: Target) {
    const value = entryQuote(next.entry, next.excerpt, next.file?.file_id)
    if (!value) return
    menuOpen = sheetOpen = false
    selection = null
    document.getSelection()?.removeAllRanges()
    onquote(value)
  }

  function download(next: Target) {
    menuOpen = sheetOpen = false
    if (next.file && next.entry.kind === 'shared_files')
      downloads.start(device.id, next.entry.session_id, { kind: 'shared', fileId: next.file.file_id }, next.file.name)
  }

  async function copy(next: Target) {
    menuOpen = sheetOpen = false
    const text = next.excerpt ?? ('content' in next.entry ? messageText(next.entry) : '')
    if (await copyText(text)) toast.success($t('common.copied'))
    else toast.error($t('files.copyFailed'))
  }

  async function selectText(next: Target) {
    sheetOpen = false
    selectableId = next.entry.id
    await tick()
    const source = root?.querySelector(`[data-bot-entry-id="${CSS.escape(next.entry.id)}"] [data-bot-quote-source]`)
    if (!source) return
    const range = document.createRange()
    range.selectNodeContents(source)
    const current = document.getSelection()!
    current.removeAllRanges()
    current.addRange(range)
  }

  // Keyboard users return to the message the menu acted on.
  function restoreFocus(event: Event) {
    event.preventDefault()
    if (target?.element.isConnected && target.element.tabIndex >= 0) target.element.focus()
  }

  const isMessage = $derived(target !== null && 'content' in target.entry)
  const quoteLabel = $derived(target?.file ? (canPreviewImage(target.file) ? $t('bots.quoteImage') : $t('bots.quoteFile')) : $t('bots.quote'))
  // Touch: below the selection, clear of the system selection menu. Mouse: above it.
  // Both stay inside the viewport when the selection runs past an edge.
  const selectionStyle = $derived.by(() => {
    if (!selection) return ''
    const { rect, touch } = selection
    const left = Math.min(Math.max(rect.left + rect.width / 2, 56), innerWidth - 56)
    const top = touch ? Math.min(rect.bottom + 48, innerHeight - 96) : Math.max(rect.top - 8, 48)
    return `left: ${left}px; top: ${top}px`
  })
</script>

<DropdownMenu.Root bind:open={menuOpen}>
  <DropdownMenu.Content customAnchor={menuAnchor} side="bottom" align="start" sideOffset={2} class="w-auto min-w-40" aria-label={$t('bots.messageActions')} onCloseAutoFocus={restoreFocus}>
    {#if target}
      {@const current = target}
      {#if current.file}
        <DropdownMenu.Label class="max-w-64 truncate text-xs font-normal text-muted-foreground" title={current.file.name}>{current.file.name}</DropdownMenu.Label>
      {/if}
      <DropdownMenu.Item onSelect={() => quote(current)}><QuotesIcon />{quoteLabel}<DropdownMenu.Shortcut>R</DropdownMenu.Shortcut></DropdownMenu.Item>
      {#if isMessage}
        <DropdownMenu.Item onSelect={() => void copy(current)}><CopyIcon />{$t('common.copy')}</DropdownMenu.Item>
      {:else if current.file}
        <DropdownMenu.Item onSelect={() => download(current)}><DownloadSimpleIcon />{$t('files.download')}</DropdownMenu.Item>
      {/if}
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

<Sheet.Root bind:open={sheetOpen}>
  <Sheet.Content side="bottom" showCloseButton={false} class="gap-0 rounded-t-2xl px-2 pt-2 pb-[max(1rem,env(safe-area-inset-bottom))]" onCloseAutoFocus={restoreFocus}>
    <div aria-hidden="true" class="mx-auto mb-2 h-1 w-9 rounded-full bg-border"></div>
    <Sheet.Title class="sr-only">{$t('bots.messageActions')}</Sheet.Title>
    {#if target}
      {@const current = target}
      {@const preview = entryQuote(current.entry, null, current.file?.file_id)}
      {#if preview}
        <Sheet.Description class="mx-2 mb-2 rounded-xl bg-muted px-3 py-2 text-[13px] leading-5"><span class="line-clamp-2 [overflow-wrap:anywhere]">{quoteSummary(preview)}</span></Sheet.Description>
      {/if}
      <div class="flex flex-col" role="menu" aria-label={$t('bots.messageActions')}>
        <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-xl px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => quote(current)}><QuotesIcon class="size-5" />{quoteLabel}</button>
        {#if current.file}
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-xl px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => download(current)}><DownloadSimpleIcon class="size-5" />{$t('files.download')}</button>
        {/if}
        {#if isMessage}
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-xl px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => void copy(current)}><CopyIcon class="size-5" />{$t('common.copy')}</button>
          <div role="separator" class="mx-2 my-1 h-px bg-border"></div>
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-xl px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => void selectText(current)}><CursorTextIcon class="size-5" />{$t('bots.selectText')}</button>
        {/if}
      </div>
    {/if}
  </Sheet.Content>
</Sheet.Root>

{#if selection && !menuOpen && !sheetOpen}
  {@const current = selection.target}
  <div role="toolbar" tabindex="-1" aria-label={$t('bots.messageActions')}
    class={['fixed z-50 flex -translate-x-1/2 gap-0.5 rounded-lg bg-popover p-0.5 text-sm text-popover-foreground shadow-md ring-1 ring-foreground/10', !selection.touch && '-translate-y-full']}
    style={selectionStyle}
    onpointerdown={event => event.preventDefault()}>
    <button type="button" class="flex items-center gap-1.5 rounded-md px-2.5 py-1 outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => quote(current)}><QuotesIcon class="size-4" />{$t('bots.quote')}</button>
    {#if !selection.touch}
      <button type="button" class="flex items-center gap-1.5 rounded-md px-2.5 py-1 outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => void copy(current)}><CopyIcon class="size-4" />{$t('common.copy')}</button>
    {/if}
  </div>
{/if}
