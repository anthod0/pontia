<script lang="ts">
  import { onDestroy } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { CheckCircle2, CircleDot, LoaderCircle, XCircle } from '@lucide/svelte'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    class?: string
    onOpen: () => void
  }

  let { steps, active = false, class: className, onOpen }: Props = $props()
  let displayedSteps = $state<Array<SessionChatThoughtStep | null>>([])
  let rolling = $state(false)
  let previousLatestStepId: string | null = null
  let currentDisplayedSteps: Array<SessionChatThoughtStep | null> = []
  let rollTimer: ReturnType<typeof setTimeout> | null = null

  const latestVisibleSteps = $derived(steps.slice(-1))
  const latestStepId = $derived(latestVisibleSteps.at(-1)?.id ?? null)
  const rows = $derived(displayedSteps.length ? displayedSteps : [null])

  function labelForStep(thoughtStep: SessionChatThoughtStep | null): string {
    if (!thoughtStep) return 'Working'
    if (thoughtStep.kind === 'thinking') return 'Thinking'
    return thoughtStep.title || (thoughtStep.kind === 'tool_call' ? 'Tool call' : 'Tool result')
  }

  function statusForStep(thoughtStep: SessionChatThoughtStep | null): string | null {
    return thoughtStep?.status ?? (active ? 'working' : null)
  }

  function contentForStep(thoughtStep: SessionChatThoughtStep | null): string {
    return thoughtStep?.content || 'Working…'
  }

  function setDisplayed(nextSteps: Array<SessionChatThoughtStep | null>): void {
    currentDisplayedSteps = nextSteps
    displayedSteps = nextSteps
  }

  function finishRolling(): void {
    setDisplayed(latestVisibleSteps)
    rolling = false
    rollTimer = null
  }

  $effect(() => {
    const nextLatestVisibleSteps = latestVisibleSteps
    const nextLatestStepId = latestStepId

    if (previousLatestStepId === null) {
      previousLatestStepId = nextLatestStepId
      setDisplayed(nextLatestVisibleSteps)
      return
    }

    if (nextLatestStepId && nextLatestStepId !== previousLatestStepId) {
      const enteringStep = nextLatestVisibleSteps.at(-1)
      const rollingSteps = enteringStep ? [...currentDisplayedSteps, enteringStep] : nextLatestVisibleSteps
      const dedupedRollingSteps = rollingSteps.filter((step, index, allSteps) => {
        if (!step) return index === allSteps.length - 1
        return allSteps.findIndex((candidate) => candidate?.id === step.id) === index
      })

      if (rollTimer) clearTimeout(rollTimer)
      setDisplayed(dedupedRollingSteps.slice(-2))
      rolling = dedupedRollingSteps.length > 1
      rollTimer = setTimeout(finishRolling, 520)
    } else if (!rolling) {
      setDisplayed(nextLatestVisibleSteps)
    }

    previousLatestStepId = nextLatestStepId
  })

  onDestroy(() => {
    if (rollTimer) clearTimeout(rollTimer)
  })
</script>

<button
  type="button"
  class={cn(
    'not-prose w-full rounded-xl pr-3 py-2.5 text-left transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    className,
  )}
  aria-label="View thought details"
  onclick={onOpen}
>
  <div class="flex min-w-0 flex-col gap-1.5">
    <div class="thought-summary-viewport min-w-0 overflow-hidden">
      <div
        data-testid="thought-summary-rolling-stack"
        class={cn('thought-summary-stack space-y-1', rolling && 'thought-summary-roll-up')}
      >
        {#each rows as row, index (row?.id ?? `working-${index}`)}
          {@const status = statusForStep(row)}
          <div class="thought-summary-row flex h-11 min-w-0 flex-col justify-center">
            <span class="flex min-w-0 items-center gap-2">
              <span class="truncate text-sm font-medium leading-5 text-foreground">{labelForStep(row)}</span>
              {#if status}
                <span class="inline-flex size-4 shrink-0 items-center justify-center text-muted-foreground" aria-label={status} title={status}>
                  {#if status === 'completed'}
                    <CheckCircle2 class="size-4" aria-hidden="true" />
                  {:else if status === 'failed' || status === 'error'}
                    <XCircle class="size-4 text-destructive" aria-hidden="true" />
                  {:else if status === 'started' || status === 'working'}
                    <LoaderCircle class="size-4 animate-spin" aria-hidden="true" />
                  {:else}
                    <CircleDot class="size-4" aria-hidden="true" />
                  {/if}
                </span>
              {/if}
              {#if active && !status}<span class="size-2 shrink-0 animate-pulse rounded-full bg-primary" aria-label="Working"></span>{/if}
            </span>
            <span class="line-clamp-1 whitespace-pre-wrap break-words text-xs leading-4 text-muted-foreground">
              {contentForStep(row)}
            </span>
          </div>
        {/each}
      </div>
    </div>
  </div>
</button>

<style>
  .thought-summary-viewport {
    --thought-summary-row-height: 2.75rem;
    max-height: var(--thought-summary-row-height);
    mask-image: linear-gradient(to bottom, transparent 0, black 0.35rem, black calc(100% - 0.35rem), transparent 100%);
  }

  .thought-summary-stack {
    transform: translateY(0);
    transform-origin: center;
    will-change: transform, filter;
  }

  .thought-summary-roll-up {
    animation: thought-summary-roll-up 520ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes thought-summary-roll-up {
    0% {
      transform: translateY(0) scaleY(1);
      filter: blur(0);
    }
    34% {
      transform: translateY(calc(var(--thought-summary-row-height) * -0.72)) scaleY(0.985);
      filter: blur(0.7px);
    }
    72% {
      transform: translateY(calc(var(--thought-summary-row-height) * -1.045)) scaleY(1.006);
      filter: blur(0);
    }
    100% {
      transform: translateY(calc(var(--thought-summary-row-height) * -1)) scaleY(1);
      filter: blur(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .thought-summary-roll-up {
      animation: none;
    }
  }
</style>
