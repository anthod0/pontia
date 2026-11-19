<script lang="ts">
  import { tick, untrack } from 'svelte'
  import type { BotAvatarColor, BotAvatarShape } from '../../api/types'
  import { cn } from '$lib/utils'
  import { avatarColors, avatarMotions, avatarShapes, type AvatarMotion } from './avatar'

  // Without `working`, the avatar stays still. With it, the avatar reacts to hover while idle,
  // wakes up into the shape's working loop, and settles once the loop returns to its still pose.
  let { shape, color, working, class: className }: {
    shape: BotAvatarShape
    color: BotAvatarColor
    working?: boolean
    class?: string
  } = $props()

  const animated = $derived(working !== undefined)
  const motion: AvatarMotion = $derived(avatarMotions[shape])
  let svg = $state<SVGSVGElement>()
  let phase = $state<'idle' | 'waking' | 'working' | 'settling'>('idle')
  let stopping = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let token: object | null = null

  $effect(() => {
    if (working === undefined) return
    untrack(step)
  })

  $effect(() => () => {
    clearTimeout(timer)
    token = null
  })

  const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches

  function smil(action: (element: SVGAnimationElement) => void) {
    svg?.querySelectorAll<SVGAnimationElement>('animate, animateTransform').forEach(action)
  }

  // Wait until the transition animation has actually finished so the next class never cuts it short.
  async function afterTransition(next: () => void) {
    const current = (token = {})
    await tick()
    try {
      await Promise.all(svg!.getAnimations({ subtree: true }).map((animation) => animation.finished))
    } catch {
      return
    }
    if (token === current) next()
  }

  function startLoop() {
    phase = 'working'
    smil((element) => element.beginElement())
  }

  function endLoop() {
    smil((element) => element.endElement())
  }

  // Milliseconds until the working loop next shows the still avatar, read from the animation clock.
  function untilRest() {
    const css = svg!.getAnimations({ subtree: true })[0]
    const element = svg!.querySelector<SVGAnimationElement>('animate')
    const elapsed = css
      ? Number(css.currentTime)
      : element ? (element.getCurrentTime() - element.getStartTime()) * 1000 : 0
    const { period, rest } = motion
    const position = (elapsed % period) / period
    if (rest.some(([start, end]) => position >= start && position <= end)) return 0
    const next = Math.min(...rest.map(([start]) => (start > position ? start : start + 1)))
    return (next - position) * period
  }

  function step() {
    if (reducedMotion()) {
      clearTimeout(timer)
      token = null
      stopping = false
      endLoop()
      phase = working ? 'working' : 'idle'
      return
    }
    if (phase === 'idle' && working) {
      phase = 'waking'
      void afterTransition(() => {
        startLoop()
        step()
      })
    } else if (phase === 'working' && !working && !stopping) {
      stopping = true
      timer = setTimeout(() => {
        stopping = false
        endLoop()
        phase = 'settling'
        void afterTransition(() => {
          phase = 'idle'
          step()
        })
      }, untilRest())
    } else if (phase === 'working' && working && stopping) {
      clearTimeout(timer)
      stopping = false
    }
  }
</script>

<svg bind:this={svg} viewBox="9.9 12.4 83 83" aria-hidden="true"
  class={cn('shrink-0 overflow-visible', animated && ['bot', shape, phase], className)}>
  <g class="body">
    <path class="shell" d={avatarShapes[shape]} fill={avatarColors[color]}
      stroke={shape === 'onigiri' ? avatarColors[color] : undefined}
      stroke-width={shape === 'onigiri' ? 28 : undefined} stroke-linejoin="round">
      {#if animated && motion.shape}
        <animate attributeName="d" begin="indefinite" repeatCount="indefinite" dur={motion.shape.dur}
          values={motion.shape.values} keyTimes={motion.shape.keyTimes} keySplines={motion.shape.keySplines}
          calcMode={motion.shape.keySplines ? 'spline' : undefined} />
      {/if}
    </path>
    <g class="eyes">
      <g fill="#fff">
        {#if animated && motion.eyes}
          <animateTransform attributeName="transform" type="translate" begin="indefinite" repeatCount="indefinite"
            dur={motion.eyes.dur} values={motion.eyes.values} />
        {/if}
        <rect class="eye" x="33" y="45" width="10" height="18" rx="5" />
        <rect class="eye" x="57" y="45" width="10" height="18" rx="5" />
      </g>
    </g>
  </g>
</svg>

<style>
  .bot .body,
  .bot .shell,
  .bot .eyes {
    transform-box: view-box;
  }

  .bot .eye {
    transform-box: fill-box;
    transform-origin: center;
  }

  .bot .eyes {
    transition: transform 160ms ease-out;
  }

  /* Hover while idle: a quick perk and blink, then the eyes widen and look up. */
  :global(a:hover) .bot.idle .body {
    transform-origin: 51px 84px;
    animation: perk 420ms cubic-bezier(0.3, 0.7, 0.4, 1.4) 1;
  }

  :global(a:hover) .bot.idle .eyes {
    transform: translateY(-2px);
  }

  :global(a:hover) .bot.idle .eye {
    animation: hover-blink 420ms ease-in-out 1 forwards;
  }

  @keyframes perk {
    30% { transform: scale(1.08, 0.9); }
    65% { transform: translateY(-4px) scale(0.96, 1.06); }
  }

  @keyframes hover-blink {
    30% { transform: scale(1.2, 0.25); }
    60%, 100% { transform: scale(1.1); }
  }

  /* Waking (idle to working): squash, spring up, and blink. */
  .bot.waking .body {
    transform-origin: 51px 84px;
    animation: wake 320ms cubic-bezier(0.3, 0.7, 0.4, 1.2) 1;
  }

  .bot.waking .eye {
    animation: blink 320ms ease-in-out 1;
  }

  @keyframes wake {
    35% { transform: scale(1.06, 0.9); }
    70% { transform: translateY(-3px) scale(0.97, 1.05); }
  }

  /* Settling (working to idle): sink a little and bounce back while blinking. */
  .bot.settling .body {
    transform-origin: 51px 84px;
    animation: settle 360ms ease-out 1;
  }

  .bot.settling .eye {
    animation: blink 360ms ease-in-out 1;
  }

  @keyframes settle {
    40% { transform: translateY(1px) scale(1.04, 0.95); }
    75% { transform: scale(0.99, 1.015); }
  }

  @keyframes blink {
    0%, 100% { transform: none; }
    45% { transform: scaleY(0.15); }
  }

  /* Working loops. Each loop starts from the still avatar. */

  /* classic: nods while reading one line per nod, blinking as the eyes return to center. */
  .working.classic .body {
    transform-origin: 51px 82px;
    animation: classic-nod 1.6s ease-in-out infinite;
  }

  .working.classic .eyes {
    animation: classic-read 1.6s ease-in-out infinite;
  }

  .working.classic .eye {
    animation: classic-blink 1.6s linear infinite;
  }

  @keyframes classic-nod {
    0%, 6%, 100% { transform: none; }
    53% { transform: rotate(3deg) translateY(1px); }
  }

  @keyframes classic-read {
    0%, 6%, 100% { transform: none; }
    20% { transform: translateX(-4px); }
    76% { transform: translateX(4px); }
  }

  @keyframes classic-blink {
    0%, 80%, 90%, 100% { transform: none; }
    85% { transform: scaleY(0.15); }
  }

  /* mochi: kneaded dough. */
  .working.mochi .body {
    transform-origin: 51px 84px;
    animation: mochi-knead 1.8s cubic-bezier(0.45, 0, 0.3, 1) infinite;
  }

  .working.mochi .eye {
    animation: mochi-squint 1.8s cubic-bezier(0.45, 0, 0.3, 1) infinite;
  }

  @keyframes mochi-knead {
    0%, 100% { transform: none; }
    30% { transform: scale(1.12, 0.84); }
    55% { transform: scale(0.94, 1.08); }
    72% { transform: scale(1.03, 0.97); }
    86% { transform: none; }
  }

  @keyframes mochi-squint {
    0%, 100% { transform: none; }
    30% { transform: scale(1.15, 0.45); }
    55% { transform: scale(0.95, 1.05); }
    72% { transform: none; }
  }

  /* cloud: pleats are pinched by the SMIL morph. */
  .working.cloud .body {
    transform-origin: 51px 82px;
    animation: cloud-squash 2.4s ease-in-out infinite;
  }

  .working.cloud .eyes {
    animation: cloud-look 2.4s ease-in-out infinite;
  }

  @keyframes cloud-squash {
    0%, 22%, 42%, 62%, 100% { transform: none; }
    10%, 30%, 50% { transform: scale(1.03, 0.97); }
  }

  @keyframes cloud-look {
    0%, 66%, 100% { transform: none; }
    10% { transform: translate(-3px, -2px); }
    30% { transform: translate(0, -2px); }
    50% { transform: translate(3px, -2px); }
  }

  /* drop: hops; the tip bends through the SMIL morph. */
  .working.drop .body {
    transform-origin: 51px 84px;
    animation: drop-hop 2.4s ease-in-out infinite;
  }

  .working.drop .eyes {
    animation: drop-eyes 2.4s ease-in-out infinite;
  }

  @keyframes drop-hop {
    0%, 44%, 50%, 94%, 100% { transform: none; }
    10%, 60% { transform: scale(1.06, 0.92); }
    22%, 72% { transform: translateY(-4px) scale(0.97, 1.04); }
    34%, 84% { transform: scale(1.02, 0.98); }
  }

  @keyframes drop-eyes {
    0%, 44%, 50%, 94%, 100% { transform: none; }
    10%, 60% { transform: translateY(1px); }
    22% { transform: translate(-1.5px, -1px); }
    72% { transform: translate(1.5px, -1px); }
  }

  /* blob: stirring is done entirely by the SMIL morphs. */

  /* ghost: drifts around, then pauses in place. */
  .working.ghost .body {
    transform-origin: 51px 56px;
    animation: ghost-drift 3.2s ease-in-out infinite;
  }

  .working.ghost .shell {
    animation: ghost-fade 3.2s ease-in-out infinite;
  }

  @keyframes ghost-drift {
    0%, 80%, 100% { transform: none; }
    20% { transform: translate(3px, -4px) rotate(5deg); }
    40% { transform: translate(0, -6px); }
    60% { transform: translate(-3px, -4px) rotate(-5deg); }
  }

  @keyframes ghost-fade {
    0%, 80%, 100% { opacity: 1; }
    40% { opacity: 0.72; }
  }

  /* onigiri: a roly-poly that slows at both ends and pauses briefly when upright. */
  .working.onigiri .body {
    transform-origin: 51px 80px;
    animation: onigiri-rock 1.6s infinite;
  }

  .working.onigiri .eyes {
    animation: onigiri-slide 1.6s infinite;
  }

  @keyframes onigiri-rock {
    0%, 4%, 50%, 54% { transform: none; animation-timing-function: cubic-bezier(0.33, 0.67, 0.67, 1); }
    27% { transform: rotate(9deg); animation-timing-function: cubic-bezier(0.33, 0, 0.67, 0.33); }
    77% { transform: rotate(-9deg); animation-timing-function: cubic-bezier(0.33, 0, 0.67, 0.33); }
    100% { transform: none; }
  }

  @keyframes onigiri-slide {
    0%, 4%, 50%, 54% { transform: none; animation-timing-function: cubic-bezier(0.33, 0.67, 0.67, 1); }
    27% { transform: translateX(3px); animation-timing-function: cubic-bezier(0.33, 0, 0.67, 0.33); }
    77% { transform: translateX(-3px); animation-timing-function: cubic-bezier(0.33, 0, 0.67, 0.33); }
    100% { transform: none; }
  }

  /* egg: about to hatch. */
  .working.egg .body {
    transform-origin: 51px 86px;
    animation: egg-shake 2.4s ease-in-out infinite;
  }

  .working.egg .eye {
    animation: egg-squeeze 2.4s ease-in-out infinite;
  }

  @keyframes egg-shake {
    0%, 32%, 100% { transform: none; }
    4% { transform: rotate(-8deg); }
    10% { transform: rotate(7deg); }
    16% { transform: rotate(-6deg); }
    22% { transform: rotate(4deg); }
    27% { transform: rotate(-2deg); }
  }

  @keyframes egg-squeeze {
    0%, 34%, 100% { transform: none; }
    3%, 28% { transform: scale(1.15, 0.35); }
  }

  /* pudding: jelly wobble. */
  .working.pudding .body {
    transform-origin: 51px 84px;
    animation: pudding-jiggle 2s ease-out infinite;
  }

  @keyframes pudding-jiggle {
    0%, 60%, 100% { transform: none; }
    8% { transform: skewX(-10deg) scaleY(0.96); }
    18% { transform: skewX(8deg) scaleY(1.03); }
    28% { transform: skewX(-5deg) scaleY(0.98); }
    38% { transform: skewX(3deg); }
    48% { transform: skewX(-1deg); }
  }

  @media (prefers-reduced-motion: reduce) {
    .bot * {
      animation: none !important;
    }
  }
</style>
