<script lang="ts">
  import { onMount } from 'svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  let { dismiss }: { dismiss: () => void } = $props()
  const stack = useMobileStack()
  let handle: HTMLElement
  onMount(() => {
    const surface = handle.parentElement!
    const id = stack?.open({dismiss: () => dismiss()})
    let start: number | null = null
    function down(event: PointerEvent) {
      if (!event.isPrimary || !(event.target instanceof Element)) return
      if (!event.target.closest('[data-sheet-drag],[data-slot$="-header"],[data-slot$="-title"]') || event.target.closest('button,input,a')) return
      start = event.clientY
      event.target.setPointerCapture(event.pointerId)
    }
    function move(event: PointerEvent) {
      if (start !== null && event.isPrimary) surface.style.translate = `0 ${Math.max(0,event.clientY - start)}px`
    }
    function end(event: PointerEvent) {
      if (start === null || !event.isPrimary) return
      const distance = event.clientY - start
      start = null
      surface.style.removeProperty('translate')
      if (event.type === 'pointerup' && distance > 80) dismiss()
    }
    surface.addEventListener('pointerdown',down)
    surface.addEventListener('pointermove',move)
    surface.addEventListener('pointerup',end)
    surface.addEventListener('pointercancel',end)
    return () => {
      surface.removeEventListener('pointerdown',down)
      surface.removeEventListener('pointermove',move)
      surface.removeEventListener('pointerup',end)
      surface.removeEventListener('pointercancel',end)
      if (id) stack?.release(id)
    }
  })
</script>
<div bind:this={handle} data-sheet-drag role="presentation" class="flex h-6 shrink-0 touch-none items-center justify-center">
  <span class="h-1 w-9 rounded-full bg-border"></span>
</div>
