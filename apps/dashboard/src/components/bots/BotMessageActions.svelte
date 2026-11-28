<script lang="ts">
  import {useMobileStack} from '$lib/mobile/overlayStack.svelte'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { selectionToolbarPosition } from '$lib/visualViewport'
  import { copyText } from '$lib/copyText'
  import { entryQuote, messageText, quoteSummary, type BotQuote } from '$lib/botQuote'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Separator } from '$lib/components/ui/separator/index.js'
  import * as Sheet from '$lib/components/ui/sheet/index.js'
  import TreeStructureIcon from 'phosphor-svelte/lib/TreeStructureIcon'
  import QuotesIcon from 'phosphor-svelte/lib/QuotesIcon'
  import ArrowBendUpLeftIcon from 'phosphor-svelte/lib/ArrowBendUpLeftIcon'
  import ListChecksIcon from 'phosphor-svelte/lib/ListChecksIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { downloads } from '$lib/downloads/manager'
  import { isImagePath } from '$lib/downloads/localFilePath'
  import { attachedFile, attachedPaths } from '$lib/attachmentBlocks'
  import { useDeviceRuntime } from '../../device/context'
  import type { AttachedFile, BotEntry } from '../../api/types'

  let { root, fileSessionId, entry, onquote, oncontext, ontodo }: {
    /** Element containing the feed rows; events are delegated from it. */
    root: HTMLElement | undefined
    /** Session through which attached files are read. */
    fileSessionId: string | undefined
    entry: (id: string) => BotEntry | undefined
    ontodo: (messageId: string, open: boolean) => Promise<void>
    oncontext: (messageId: string) => void
    onquote: (quote: BotQuote) => void
  } = $props()

  /** A message, or one file attached to it when `file` is set. */
  type Target = { entry: BotEntry; element: HTMLElement; excerpt: string | null; file: AttachedFile | null }
  const stack = useMobileStack()
  const device = useDeviceRuntime()

  let target = $state<Target | null>(null)
  /** The row's menu button that opened the menu, if any. */
  let opener = $state<HTMLElement | null>(null)
  let menuOpen = $state(false)
  let sheetOpen = $state(false)
  let anchor = $state({ x: 0, y: 0 })
  let selection = $state<{ target: Target; rect: DOMRect; touch: boolean } | null>(null)
  let toolbarWidth = $state(0)
  let toolbarHeight = $state(0)
  let touchSelection = false
  let pointerDown = false

  const menuAnchor = { getBoundingClientRect: () => DOMRect.fromRect({ x: anchor.x, y: anchor.y, width: 0, height: 0 }) }

  type Row = Omit<Target, 'excerpt'>
  // Files are quoted one at a time: each attached file is its own quotable element.
  function rowOf(node: Node | null): Row | null {
    const element = (node instanceof Element ? node : node?.parentElement)?.closest<HTMLElement>('[data-bot-quotable]')
    const id = element?.closest<HTMLElement>('[data-bot-entry-id]')?.dataset.botEntryId
    const found = id ? entry(id) : undefined
    if (!element || !found || !root?.contains(element)) return null
    const path = element.dataset.botFilePath
    const file = path && found.kind === 'bot_reply' && attachedPaths(found.content).includes(path) ? attachedFile(path) : null
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

  // An attached image opens the file menu; other images and links keep the browser's.
  function keepsNativeMenu(event: MouseEvent): boolean {
    if (event.shiftKey || !(event.target instanceof Element)) return event.shiftKey
    const native = event.target.closest('a, img, video, input, textarea, [contenteditable="true"]')
    return Boolean(native && !(native.tagName === 'IMG' && native.closest('[data-bot-file-path]')))
  }

  function open(next: Target, touch: boolean, point: { x: number; y: number }, from: HTMLElement | null = null) {
    opener = from
    selection = null
    target = next
    anchor = point
    if (touch) sheetOpen = true
    else menuOpen = true
  }

  function contextmenu(event: MouseEvent) {
    const row = rowOf(event.target as Node)
    // A long press on touch keeps the native text selection; touch users open the menu with the row's button.
    if (!row || keepsNativeMenu(event) || touchSelection) return
    event.preventDefault()
    const selected = selectedTarget()
    open(selected?.element === row.element ? selected : { ...row, excerpt: null }, false, { x: event.clientX, y: event.clientY })
  }

  function pointerdown(event: PointerEvent) {
    pointerDown = true
    touchSelection = event.pointerType === 'touch'
  }

  function pointerup() {
    pointerDown = false
    updateSelection()
  }

  /** Opens a row's menu from its menu button: a sheet on touch, a dropdown with a mouse or keyboard. */
  export function openMenu(id: string, event: MouseEvent) {
    const button = event.currentTarget as HTMLElement
    const own = button.closest<HTMLElement>('[data-bot-quotable]')
    const element = own ?? button.closest('[data-bot-entry-id]')?.querySelector<HTMLElement>('[data-bot-quotable]')
    const found = rowOf(element ?? null)
    if (!found || found.entry.id !== id) return
    // A button outside every quotable element acts on the whole message, even when only its files are quotable.
    const row = own ? found : { ...found, file: null }
    const rect = button.getBoundingClientRect()
    const touch = !!stack || (event instanceof PointerEvent && event.pointerType === 'touch') || matchMedia('(pointer: coarse)').matches
    open({ ...row, excerpt: null }, touch, { x: rect.left, y: rect.bottom }, button)
  }

  function updateSelection() {
    if (pointerDown && !touchSelection) return
    const selected = selectedTarget()
    if (!selected) {
      selection = null
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
      ['pointerup', pointerup],
      ['pointercancel', pointerup],
      ['keydown', keydown as EventListener],
      ['scroll', updateSelection, true],
    ]
    for (const [type, listener, capture] of listeners) element.addEventListener(type, listener, capture)
    const viewport = window.visualViewport
    viewport?.addEventListener('resize', updateSelection)
    viewport?.addEventListener('scroll', updateSelection)
    document.addEventListener('selectionchange', updateSelection)
    return () => {
      for (const [type, listener, capture] of listeners) element.removeEventListener(type, listener, capture)
      viewport?.removeEventListener('resize', updateSelection)
      viewport?.removeEventListener('scroll', updateSelection)
      document.removeEventListener('selectionchange', updateSelection)
    }
  })

  function quote(next: Target) {
    const value = entryQuote(next.entry, next.excerpt, next.file?.path)
    if (!value) return
    menuOpen = sheetOpen = false
    selection = null
    document.getSelection()?.removeAllRanges()
    onquote(value)
  }

  function viewContext(next: Target) {
    if(sheetOpen && stack){stack.back(()=>oncontext(next.entry.id));return}
    menuOpen = sheetOpen = false
    oncontext(next.entry.id)
  }

  let pending = $state<string[]>([])
  async function toggleTodo(next: Target) {
    if (pending.includes(next.entry.id)) return
    pending.push(next.entry.id)
    menuOpen = sheetOpen = false
    try { await ontodo(next.entry.id, !openTodo) } finally { pending = pending.filter(id => id !== next.entry.id) }
  }

  function download(next: Target) {
    menuOpen = sheetOpen = false
    if (next.file && fileSessionId)
      downloads.start(device.id, fileSessionId, { path: next.file.path }, next.file.name)
  }

  async function copy(next: Target) {
    menuOpen = sheetOpen = false
    const text = next.excerpt ?? ('content' in next.entry ? messageText(next.entry) : '')
    if (await copyText(text)) toast.success($t('common.copied'), { duration: 1500 })
    else toast.error($t('files.copyFailed'), { duration: 6000 })
  }

  // The menu button reports whether its menu is open; the page styles it from that.
  $effect(() => {
    const button = opener
    if (!button || !(menuOpen || sheetOpen)) return
    button.setAttribute('aria-expanded', 'true')
    return () => button.removeAttribute('aria-expanded')
  })

  // Keyboard users return to the message the menu acted on, or to the button that opened the dropdown.
  function restoreFocus(event: Event, toOpener = false) {
    event.preventDefault()
    if (toOpener && opener?.isConnected) opener.focus()
    else if (target?.element.isConnected && target.element.tabIndex >= 0) target.element.focus()
  }

  const row = $derived(target && entry(target.entry.id))
  const todoMessage = $derived(row && 'content' in row && row.status === 'normal' ? row : null)
  const openTodo = $derived(todoMessage?.todo?.state === 'open')
  const isMessage = $derived(target !== null && !target.file && 'content' in target.entry)
  // A whole message is quoted as a reply; a file keeps its quote label.
  const quoteLabel = $derived(target?.file ? (isImagePath(target.file.path) ? $t('bots.quoteImage') : $t('bots.quoteFile')) : $t('bots.reply'))
  const QuoteIcon = $derived(target?.file ? QuotesIcon : ArrowBendUpLeftIcon)
  // Mobile keeps the quote action above the composer. Desktop positions it beside the selection.
  // Both stay inside the viewport when the selection runs past an edge.
  const selectionStyle = $derived.by(() => {
    if (!selection) return ''
    const { rect, touch } = selection
    if (stack) return 'left:50%; bottom:calc(var(--mobile-footer-h, 60px) + var(--visual-viewport-bottom, 0px) + 20px)'
    const viewport = window.visualViewport
    const x = viewport?.offsetLeft ?? 0
    const y = viewport?.offsetTop ?? 0
    const width = viewport?.width ?? window.innerWidth
    const height = viewport?.height ?? window.innerHeight
    const { left, top } = selectionToolbarPosition(rect, touch, { x, y, width, height }, toolbarWidth, toolbarHeight)
    return `left: ${left}px; top: ${top}px`
  })
</script>

<DropdownMenu.Root bind:open={menuOpen}>
  <DropdownMenu.Content customAnchor={menuAnchor} side="bottom" align="start" sideOffset={2} class="w-auto min-w-40" aria-label={$t('bots.messageActions')} onCloseAutoFocus={event => restoreFocus(event, true)}>
    {#if target}
      {@const current = target}
      {#if current.file}
        <DropdownMenu.Label class="max-w-64 truncate text-xs font-normal text-muted-foreground" title={current.file.name}>{current.file.name}</DropdownMenu.Label>
      {/if}
      <DropdownMenu.Item onSelect={() => quote(current)}><QuoteIcon />{quoteLabel}<DropdownMenu.Shortcut>R</DropdownMenu.Shortcut></DropdownMenu.Item>
      {#if isMessage}
        <DropdownMenu.Item onSelect={() => void copy(current)}><CopyIcon />{$t('common.copy')}</DropdownMenu.Item>
        <DropdownMenu.Item onSelect={() => viewContext(current)}><TreeStructureIcon />{$t('bots.viewContext')}</DropdownMenu.Item>
        {#if todoMessage}
          <DropdownMenu.Separator />
          <DropdownMenu.Item disabled={pending.includes(current.entry.id)} onSelect={() => toggleTodo(current)}><ListChecksIcon />{$t(openTodo ? 'bots.cancelTodo' : 'bots.markTodo')}</DropdownMenu.Item>
        {/if}
      {:else if current.file}
        <DropdownMenu.Item onSelect={() => download(current)}><DownloadSimpleIcon />{$t('files.download')}</DropdownMenu.Item>
      {/if}
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

<Sheet.Root bind:open={sheetOpen}>
  <BottomSheet dismiss={() => sheetOpen = false} showCloseButton={false} class="px-3" onCloseAutoFocus={restoreFocus}>
    <Sheet.Title class="sr-only">{$t('bots.messageActions')}</Sheet.Title>
    {#if target}
      {@const current = target}
      {@const preview = entryQuote(current.entry, null, current.file?.path)}
      {#if preview}
        <Sheet.Description class="mx-2 mb-2 rounded-item bg-muted px-3 py-2 text-13 leading-5"><span class="line-clamp-2 [overflow-wrap:anywhere]">{quoteSummary(preview)}</span></Sheet.Description>
      {/if}
      <div class="flex flex-col" role="menu" aria-label={$t('bots.messageActions')}>
        <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-item px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => quote(current)}><QuoteIcon class="size-5" />{quoteLabel}</button>
        {#if current.file}
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-item px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => download(current)}><DownloadSimpleIcon class="size-5" />{$t('files.download')}</button>
        {/if}
        {#if isMessage}
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-item px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => void copy(current)}><CopyIcon class="size-5" />{$t('common.copy')}</button>
          <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-item px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => viewContext(current)}><TreeStructureIcon class="size-5" />{$t('bots.viewContext')}</button>
          {#if todoMessage}
            <Separator class="mx-2 my-1 data-[orientation=horizontal]:w-auto" />
            <button type="button" role="menuitem" class="flex items-center gap-3.5 rounded-item px-3.5 py-3 text-base outline-none hover:bg-muted focus-visible:bg-muted" disabled={pending.includes(current.entry.id)} onclick={() => toggleTodo(current)}><ListChecksIcon class="size-5" />{$t(openTodo ? 'bots.cancelTodo' : 'bots.markTodo')}</button>
          {/if}
        {/if}
      </div>
    {/if}
  </BottomSheet>
</Sheet.Root>

{#if selection && !menuOpen && !sheetOpen}
  {@const current = selection.target}
  <div bind:clientWidth={toolbarWidth} bind:clientHeight={toolbarHeight} role="toolbar" tabindex="-1" aria-label={$t('bots.messageActions')}
    data-mobile-toast-avoid={stack ? '' : undefined}
    class={['fixed z-(--layer-toolbar) flex -translate-x-1/2 gap-0.5 rounded-item bg-popover p-0.5 text-sm text-popover-foreground shadow-md border border-border', !stack && !selection.touch && '-translate-y-full']}
    style={selectionStyle}
    onpointerdown={event => event.preventDefault()}>
    <button type="button" class="flex pointer-coarse:min-h-11 items-center gap-1.5 rounded-item px-2.5 py-1 outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => quote(current)}><QuotesIcon class="size-4" />{$t(stack ? 'bots.quoteSelection' : 'bots.quote')}</button>
    {#if !stack && !selection.touch}
      <button type="button" class="flex pointer-coarse:min-h-11 items-center gap-1.5 rounded-item px-2.5 py-1 outline-none hover:bg-muted focus-visible:bg-muted" onclick={() => void copy(current)}><CopyIcon class="size-4" />{$t('common.copy')}</button>
    {/if}
  </div>
{/if}
