<script lang="ts">
  import type { Snippet } from 'svelte'
  import FileArrowUpIcon from 'phosphor-svelte/lib/FileArrowUpIcon'
  import { t } from '$lib/i18n'
  import { provideFileDrop } from './fileDrop.svelte'

  let { children }: { children: Snippet } = $props()
  const fileDrop = provideFileDrop()
  let zone = $state<HTMLElement>()

  // Nested elements fire their own enter/leave pairs; count them so the overlay stays up until the drag leaves the zone.
  // A leave toward the outside resets the count, since elements re-rendered mid-drag never send their own leave.
  let dragDepth = 0
  // The mask covers the conversation and stops above the input box, which stays sharp as the drop destination.
  // The prompt sits just above the input box and matches its width.
  let inputBox = $state({ top: 0, left: 0, width: 0 })
  const unavailableReason = $derived(fileDrop.target?.unavailableReason ?? null)

  function setDragDepth(depth: number): void {
    dragDepth = depth
    fileDrop.dragging = depth > 0
  }
  // The Bot side panel renders inside the zone but sits beside the main panel, outside the overlay.
  function inMainPanel(node: EventTarget | null): boolean {
    return node instanceof Element && zone!.contains(node) && !node.closest('[data-bot-chat-panel]')
  }
  function droppable(event: DragEvent): boolean {
    return fileDrop.target !== null && Boolean(event.dataTransfer?.types.includes('Files')) && inMainPanel(event.target)
  }
  function dragEnter(event: DragEvent): void {
    if (!droppable(event)) return
    event.preventDefault()
    if (dragDepth === 0) {
      const rect = fileDrop.target!.element?.getBoundingClientRect()
      inputBox = rect ? { top: rect.top, left: rect.left, width: rect.width } : { top: window.innerHeight, left: 0, width: 0 }
    }
    setDragDepth(dragDepth + 1)
  }
  function dragOver(event: DragEvent): void {
    if (!droppable(event)) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = fileDrop.target!.unavailableReason ? 'none' : 'copy'
  }
  function dragLeave(event: DragEvent): void {
    if (!droppable(event)) return
    const next = event.relatedTarget
    setDragDepth(next instanceof Node && !inMainPanel(next) ? 0 : Math.max(0, dragDepth - 1))
  }
  // Capture keeps the message editor from inserting the dropped file as text.
  function drop(event: DragEvent): void {
    if (!droppable(event)) return
    event.preventDefault()
    event.stopPropagation()
    setDragDepth(0)
    const target = fileDrop.target!
    if (target.unavailableReason || !event.dataTransfer!.files.length) return
    target.add(Array.from(event.dataTransfer!.files))
  }
</script>

<div bind:this={zone} class="contents" role="presentation" ondragenter={dragEnter} ondragover={dragOver} ondragleave={dragLeave} ondropcapture={drop}>
  {@render children()}
  {#if fileDrop.dragging && fileDrop.target}
    {@const inputBottom = Math.max(0, window.innerHeight - inputBox.top)}
    <div class="pointer-events-none fixed top-0 left-0 right-0 z-45 bg-background/70 backdrop-blur-[3px] transition-[left] duration-200 ease-linear md:left-[var(--sidebar-width)] lg:right-[var(--file-preview-width,0px)] group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]" style:bottom={`${inputBottom}px`}></div>
    <div class={['pointer-events-none fixed z-45 flex items-center gap-3 rounded-xl border-[1.5px] border-dashed px-3.5 py-3 shadow-sm', unavailableReason ? 'border-muted-foreground/40 bg-muted' : 'border-primary/55 bg-selected']} style:left={`${inputBox.left}px`} style:width={`${inputBox.width}px`} style:bottom={`${inputBottom + 8}px`}>
      <span class={['grid size-9 shrink-0 place-items-center rounded-full', unavailableReason ? 'bg-background text-inactive' : 'bg-primary/14 text-primary']}>
        <FileArrowUpIcon class="size-5" weight="duotone" />
      </span>
      <div class="min-w-0">
        <p class={['text-sm font-semibold', unavailableReason ? 'text-muted-foreground' : 'text-heading']}>
          {unavailableReason ? $t('files.dropUnavailable') : $t('files.dropToAdd')}
        </p>
        <p class="text-xs text-muted-foreground">{unavailableReason ?? $t('files.dropHint')}</p>
      </div>
    </div>
  {/if}
</div>
