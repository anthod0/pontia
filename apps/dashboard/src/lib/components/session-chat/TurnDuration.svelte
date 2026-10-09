<script lang="ts">
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
  const visible = $derived(
    startedAt !== null && endAt !== null && (running || turn?.state === 'completed'),
  )
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
    if (hours) return `${hours}h ${minutes}m ${seconds}s`
    if (minutes) return `${minutes}m ${seconds}s`
    return `${seconds}s`
  }
</script>

<span
  class="inline-flex items-center gap-1"
  role={running ? 'timer' : 'status'}
  aria-label={running ? 'Turn running time' : 'Turn completed duration'}
  aria-live={running ? 'off' : 'polite'}
  data-chat-turn-duration
>
  <span>{running ? 'working for' : 'worked for'}</span>
  {#if visible}
    <time datetime={`PT${elapsedSeconds}S`}>{formatDuration(elapsedSeconds)}</time>
  {:else}
    <span>…</span>
  {/if}
</span>
