<script lang="ts">
  import { t } from '$lib/i18n';
  import type { TurnView } from '../../../api/types'

  type DurationTurn = Pick<TurnView, 'state' | 'started_at' | 'completed_at'>

  interface Props {
    turn?: DurationTurn
    active?: boolean
  }

  let { turn, active = false }: Props = $props()
  let now = $state(Date.now())
  const running = $derived(turn ? turn.state === 'running' : active)
  const startedAt = $derived(timestamp(turn?.started_at ?? null))
  const completedAt = $derived(timestamp(turn?.completed_at ?? null))
  const endAt = $derived(running ? now : completedAt)
  const visible = $derived(startedAt !== null && endAt !== null)
  const elapsedSeconds = $derived(
    visible ? Math.max(0, Math.floor((endAt! - startedAt!) / 1000)) : 0,
  )

  $effect(() => {
    if (!running) return
    now = Date.now()
    const interval = window.setInterval(() => {
      now = Date.now()
    }, 1000)
    return () => window.clearInterval(interval)
  })

  function timestamp(value: string | null): number | null {
    if (!value) return null
    const parsed = Date.parse(value)
    return Number.isFinite(parsed) ? parsed : null
  }

  function formatDuration(totalSeconds: number): string {
    const hours = Math.floor(totalSeconds / 3600)
    const minutes = Math.floor((totalSeconds % 3600) / 60)
    const seconds = totalSeconds % 60
    if (hours) return $t('chat.hoursMinutesSeconds', { hours, minutes, seconds })
    if (minutes) return $t('chat.minutesSeconds', { minutes, seconds })
    return $t('chat.seconds', { seconds })
  }
</script>

<span
  class="inline-flex items-center gap-1"
  role={running ? 'timer' : 'status'}
  aria-label={running ? $t('chat.runningTime') : $t('chat.completedDuration')}
  aria-live={running ? 'off' : 'polite'}
  data-chat-turn-duration
>
  {#if running}
    <span class="working-bars" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
    </span>
  {/if}
  <span>{running ? $t('chat.workingFor') : $t('chat.workedFor')}</span>
  {#if visible}
    <time datetime={`PT${elapsedSeconds}S`}>{formatDuration(elapsedSeconds)}</time>
  {:else}
    <span>…</span>
  {/if}
</span>

<style>
  .working-bars {
    display: inline-flex;
    width: 12px;
    height: 12px;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }

  .working-bars > span {
    width: 2px;
    height: 10px;
    transform: scaleY(0.3);
    animation: working-bar 1.1s ease-in-out infinite;
    border-radius: 1px;
    background: currentColor;
  }

  .working-bars > span:nth-child(1) {
    animation-delay: -0.44s;
  }

  .working-bars > span:nth-child(2) {
    animation-delay: -0.22s;
  }

  @keyframes working-bar {
    0%,
    100% {
      transform: scaleY(0.3);
    }

    40% {
      transform: scaleY(1);
    }

    70% {
      transform: scaleY(0.45);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .working-bars > span {
      animation: none;
    }

    .working-bars > span:nth-child(1) {
      transform: scaleY(0.5);
    }

    .working-bars > span:nth-child(2) {
      transform: scaleY(1);
    }

    .working-bars > span:nth-child(3) {
      transform: scaleY(0.7);
    }
  }
</style>
