<script lang="ts">
  import type { TurnView } from '../../../api/types'

  type DurationTurn = Pick<TurnView, 'state' | 'started_at' | 'completed_at'>

  interface Props {
    turn: DurationTurn
  }

  let { turn }: Props = $props()
  let now = $state(Date.now())
  const running = $derived(turn.state === 'running')
  const startedAt = $derived(timestamp(turn.started_at))
  const completedAt = $derived(timestamp(turn.completed_at))
  const endAt = $derived(running ? now : completedAt)
  const visible = $derived(
    startedAt !== null && endAt !== null && (running || turn.state === 'completed'),
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

{#if visible}
  <div
    class="not-prose -mt-3 flex items-center gap-1.5 text-xs text-muted-foreground"
    role={running ? 'timer' : 'status'}
    aria-label={running ? 'Turn running time' : 'Turn completed duration'}
    aria-live={running ? 'off' : 'polite'}
    data-chat-turn-duration
  >
    <span>{running ? 'Running' : 'Completed'}</span>
    <span aria-hidden="true">·</span>
    <time datetime={`PT${elapsedSeconds}S`}>{formatDuration(elapsedSeconds)}</time>
  </div>
{/if}
