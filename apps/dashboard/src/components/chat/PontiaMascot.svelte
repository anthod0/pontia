<script lang="ts">
  import { onMount } from 'svelte'

  let { class: className = '', mood = 'idle' }: { class?: string; mood?: 'idle' | 'loading' | 'offline' } = $props()
  const loading = $derived(mood === 'loading')
  const offline = $derived(mood === 'offline')

  const MAX_X = 4
  const MAX_Y = 3
  // Pointer distance (px) at which the eyes reach half of their travel.
  const FALLOFF = 120

  let svg: SVGSVGElement
  let gazeX = $state(0)
  let gazeY = $state(0)

  onMount(() => {
    if (mood !== 'idle' || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

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

<svg bind:this={svg} xmlns="http://www.w3.org/2000/svg" viewBox="9.9 12.4 83 83" class={[className, mood !== 'idle' && mood]} aria-hidden="true">
  {#if mood !== 'idle'}
    <g class="shadow"><ellipse fill="currentColor" cx="51" cy="90" rx="26" ry="3" /></g>
  {/if}
  <g class="body">
    <path fill="#458588" d="M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z" />
    <g class="gaze" style:transform={`translate(${gazeX}px, ${gazeY}px)`}>
      <rect class="eye" fill="#fff" x="33" y="45" width="10" height="18" rx="5" />
      <rect class="eye" fill="#fff" x="57" y="45" width="10" height="18" rx="5" />
    </g>
    {#if loading}
      <g class="scan">
        <rect fill="#fff" x="33" y="45" width="10" height="18" rx="5" class="eye" />
        <rect fill="#fff" x="57" y="45" width="10" height="18" rx="5" class="eye" />
      </g>
    {:else if offline}
      <g fill="#fff">
        <rect x="32" y="55" width="12" height="4" rx="2" />
        <rect x="56" y="55" width="12" height="4" rx="2" />
      </g>
    {/if}
  </g>
  {#if loading}
    <g class="signal" fill="none" stroke="#458588" stroke-width="2" stroke-linecap="round">
      <path d="M82.44 23.02A5 5 0 0 1 86.98 27.56" />
      <path d="M82.78 19.03A9 9 0 0 1 90.97 27.22" />
      <path d="M83.13 15.05A13 13 0 0 1 94.95 26.87" />
    </g>
  {:else if offline}
    <g class="signal-lost" fill="none" stroke-width="2" stroke-linecap="round">
      <g class="arcs">
        <path d="M82.44 23.02A5 5 0 0 1 86.98 27.56" />
        <path d="M82.78 19.03A9 9 0 0 1 90.97 27.22" />
        <path d="M83.13 15.05A13 13 0 0 1 94.95 26.87" />
      </g>
      <path class="slash" d="M81 14L95 28" />
    </g>
  {/if}
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

  /* Loading: the mascot hops while its eyes search side to side and a signal radiates from its top-right. */
  .loading,
  .offline {
    overflow: visible;
  }

  .loading .gaze {
    display: none;
  }

  .loading .body {
    animation: hop 1.2s ease-in-out infinite;
  }

  .scan {
    animation: scan 2.4s ease-in-out infinite;
  }

  .shadow {
    color: var(--color-muted-foreground);
    opacity: 0.2;
    transform-box: fill-box;
    transform-origin: center;
    animation: shadow 1.2s ease-in-out infinite;
  }

  /* Signal arcs fade in from the inside out, then fade out together before the loop restarts. */
  .signal path {
    opacity: 0;
    animation: 2.4s ease-in-out infinite;
  }

  .signal path:nth-child(1) {
    animation-name: signal-1;
  }

  .signal path:nth-child(2) {
    animation-name: signal-2;
  }

  .signal path:nth-child(3) {
    animation-name: signal-3;
  }

  @keyframes signal-1 {
    0% {
      opacity: 0;
    }
    15%, 75% {
      opacity: 1;
    }
    90%, 100% {
      opacity: 0;
    }
  }

  @keyframes signal-2 {
    0%, 20% {
      opacity: 0;
    }
    35%, 75% {
      opacity: 1;
    }
    90%, 100% {
      opacity: 0;
    }
  }

  @keyframes signal-3 {
    0%, 40% {
      opacity: 0;
    }
    55%, 75% {
      opacity: 1;
    }
    90%, 100% {
      opacity: 0;
    }
  }

  /* Offline: the mascot closes its eyes and breathes slowly beside a crossed-out signal. */
  .offline .gaze {
    display: none;
  }

  .offline .body {
    animation-duration: 5s;
  }

  .offline .shadow {
    animation: none;
  }

  .signal-lost .arcs {
    stroke: #458588;
    opacity: 0.4;
  }

  .signal-lost .slash {
    stroke: var(--color-destructive);
  }

  @keyframes hop {
    0%, 100% {
      transform: none;
    }
    15% {
      transform: scale(1.04, 0.95);
    }
    45% {
      transform: translateY(-5px) scale(0.98, 1.03);
    }
    75% {
      transform: scale(1.03, 0.97);
    }
  }

  @keyframes shadow {
    45% {
      transform: scale(0.8);
      opacity: 0.1;
    }
  }

  @keyframes scan {
    0%, 100% {
      transform: translateX(-4px);
    }
    40%, 50% {
      transform: translateX(4px);
    }
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
    .eye,
    .loading .body,
    .scan,
    .shadow,
    .signal path {
      animation: none;
    }

    .signal path {
      opacity: 1;
    }
  }
</style>
