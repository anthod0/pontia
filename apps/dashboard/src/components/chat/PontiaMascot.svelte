<script lang="ts">
  import { onMount } from 'svelte'

  let { class: className = '' }: { class?: string } = $props()

  const MAX_X = 4
  const MAX_Y = 3
  // Pointer distance (px) at which the eyes reach half of their travel.
  const FALLOFF = 120

  let svg: SVGSVGElement
  let gazeX = $state(0)
  let gazeY = $state(0)

  onMount(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

    let frame = 0
    let pointerX = 0
    let pointerY = 0

    const update = () => {
      frame = 0
      const rect = svg.getBoundingClientRect()
      const dx = pointerX - (rect.left + rect.width / 2)
      const dy = pointerY - (rect.top + rect.height * 0.6)
      const distance = Math.hypot(dx, dy)
      if (!distance) {
        gazeX = 0
        gazeY = 0
        return
      }
      const reach = distance / (distance + FALLOFF)
      gazeX = (dx / distance) * reach * MAX_X
      gazeY = (dy / distance) * reach * MAX_Y
    }

    const handlePointerMove = (event: PointerEvent) => {
      pointerX = event.clientX
      pointerY = event.clientY
      if (!frame) frame = requestAnimationFrame(update)
    }

    window.addEventListener('pointermove', handlePointerMove, { passive: true })
    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      if (frame) cancelAnimationFrame(frame)
    }
  })
</script>

<svg bind:this={svg} xmlns="http://www.w3.org/2000/svg" viewBox="9.9 12.4 83 83" class={className} aria-hidden="true">
  <g class="body">
    <path fill="#458588" d="M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z" />
    <g class="gaze" style:transform={`translate(${gazeX}px, ${gazeY}px)`}>
      <rect class="eye" fill="#fff" x="33" y="45" width="10" height="18" rx="5" />
      <rect class="eye" fill="#fff" x="57" y="45" width="10" height="18" rx="5" />
    </g>
  </g>
</svg>

<style>
  .body {
    transform-box: view-box;
    transform-origin: 51px 83px;
    animation: breathe 3s ease-in-out infinite;
  }

  .gaze {
    transition: transform 120ms ease-out;
  }

  .eye {
    transform-box: fill-box;
    transform-origin: center;
    animation: blink 6s linear infinite;
  }

  @keyframes breathe {
    50% {
      transform: scale(1.015, 1.03);
    }
  }

  @keyframes blink {
    0%, 28%, 32%, 75%, 79%, 81%, 85%, 100% {
      transform: none;
    }
    30%, 77%, 83% {
      transform: scale(1.25, 0.3);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .body,
    .eye {
      animation: none;
    }
  }
</style>
