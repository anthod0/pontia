<script lang="ts">
  let { dismiss }: { dismiss: () => void } = $props()
  let start: number | null = null
  function down(event: PointerEvent) {
    if (!event.isPrimary) return
    start = event.clientY
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }
  function move(event: PointerEvent) {
    if (start === null || !event.isPrimary) return
    const surface = (event.currentTarget as HTMLElement).parentElement!
    surface.style.translate = `0 ${Math.max(0, event.clientY - start)}px`
  }
  function end(event: PointerEvent) {
    if (start === null || !event.isPrimary) return
    const distance = event.clientY - start
    start = null
    ;(event.currentTarget as HTMLElement).parentElement!.style.removeProperty('translate')
    if (event.type === 'pointerup' && distance > 80) dismiss()
  }
</script>

<div role="presentation" class="flex h-6 shrink-0 touch-none items-center justify-center" onpointerdown={down} onpointermove={move} onpointerup={end} onpointercancel={end}>
  <span class="h-1 w-9 rounded-full bg-border"></span>
</div>
