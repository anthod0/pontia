<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import { clampFilePreviewWidth, filePreviewWidth, FILE_PREVIEW_MIN_WIDTH } from './filePreviewWidth'

  const keyboardStep = 32
  let handle: HTMLDivElement
  let drag = $state<{ pointerId: number; startX: number; startWidth: number } | null>(null)
  let currentWidth = $state<number | null>(null)

  onMount(() => {
    const observer = new ResizeObserver(() => { currentWidth = Math.round(panelWidth()) })
    observer.observe(handle.parentElement!)
    currentWidth = Math.round(panelWidth())
    return () => observer.disconnect()
  })

  $effect(() => {
    if (!drag) return
    document.body.classList.add('file-preview-resizing')
    return () => document.body.classList.remove('file-preview-resizing')
  })

  function panelWidth(): number {
    return handle.parentElement!.getBoundingClientRect().width
  }

  function resize(width: number): number {
    const space = handle.closest('.file-preview-layout')!
    const next = clampFilePreviewWidth(width, space.getBoundingClientRect().width)
    currentWidth = next
    filePreviewWidth.preview(next)
    return next
  }

  function start(event: PointerEvent) {
    if (event.button !== 0) return
    event.preventDefault()
    handle.setPointerCapture(event.pointerId)
    drag = { pointerId: event.pointerId, startX: event.clientX, startWidth: panelWidth() }
  }

  function move(event: PointerEvent) {
    if (drag?.pointerId !== event.pointerId) return
    resize(drag.startWidth + drag.startX - event.clientX)
  }

  function end(event: PointerEvent) {
    if (drag?.pointerId !== event.pointerId) return
    drag = null
    filePreviewWidth.save(Math.round(panelWidth()))
  }

  function keydown(event: KeyboardEvent) {
    const direction = event.key === 'ArrowLeft' ? 1 : event.key === 'ArrowRight' ? -1 : 0
    if (!direction) return
    event.preventDefault()
    filePreviewWidth.save(resize(panelWidth() + direction * keyboardStep))
  }

  function reset() {
    filePreviewWidth.save(null)
  }
</script>

<!-- A focusable separator is the WAI-ARIA window splitter pattern. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={handle}
  role="separator"
  tabindex="0"
  aria-orientation="vertical"
  aria-label={$t('files.resizePreview')}
  aria-valuemin={Math.min(FILE_PREVIEW_MIN_WIDTH, currentWidth ?? FILE_PREVIEW_MIN_WIDTH)}
  aria-valuenow={currentWidth ?? undefined}
  title={$t('files.resizePreview')}
  data-dragging={drag ? 'true' : undefined}
  class="group absolute inset-y-0 -left-1.5 z-10 hidden w-3 cursor-col-resize touch-none outline-none lg:block"
  onpointerdown={start}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={end}
  onkeydown={keydown}
  ondblclick={reset}
>
  <span class="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-transparent transition-colors group-hover:bg-primary/40 group-focus-visible:bg-primary group-data-[dragging=true]:bg-primary"></span>
</div>

<style>
  :global(body.file-preview-resizing) {
    cursor: col-resize;
    user-select: none;
  }
</style>
