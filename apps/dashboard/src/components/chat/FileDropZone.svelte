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
  let dragDepth = $state(0)
  const showOverlay = $derived(dragDepth > 0 && fileDrop.target !== null && !fileDrop.target.unavailable)

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
    dragDepth += 1
  }
  function dragOver(event: DragEvent): void {
    if (!droppable(event)) return
    event.preventDefault()
    event.dataTransfer!.dropEffect = fileDrop.target!.unavailable ? 'none' : 'copy'
  }
  function dragLeave(event: DragEvent): void {
    if (!droppable(event)) return
    const next = event.relatedTarget
    dragDepth = next instanceof Node && !inMainPanel(next) ? 0 : Math.max(0, dragDepth - 1)
  }
  // Capture keeps the message editor from inserting the dropped file as text.
  function drop(event: DragEvent): void {
    if (!droppable(event)) return
    event.preventDefault()
    event.stopPropagation()
    dragDepth = 0
    const target = fileDrop.target!
    if (target.unavailable || !event.dataTransfer!.files.length) return
    target.add(Array.from(event.dataTransfer!.files))
  }
</script>

<div bind:this={zone} class="contents" role="presentation" ondragenter={dragEnter} ondragover={dragOver} ondragleave={dragLeave} ondropcapture={drop}>
  {@render children()}
  {#if showOverlay}
    <div class="pointer-events-none fixed inset-y-0 left-0 right-0 z-45 flex flex-col items-center justify-center gap-3 bg-background/60 text-primary backdrop-blur-sm transition-[left] duration-200 ease-linear md:left-[var(--sidebar-width)] lg:right-[var(--file-preview-width,0px)] group-has-data-[state=collapsed]/sidebar-wrapper:md:left-[var(--sidebar-width-icon)]">
      <FileArrowUpIcon class="size-10" />
      <p class="text-base font-medium">{$t('files.dropToUpload')}</p>
    </div>
  {/if}
</div>
