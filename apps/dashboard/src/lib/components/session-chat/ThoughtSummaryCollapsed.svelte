<script lang="ts">
  import { cn } from '$lib/utils.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    step: SessionChatThoughtStep | null
    stepCount: number
    active?: boolean
    class?: string
    onOpen: () => void
  }

  let { step, stepCount, active = false, class: className, onOpen }: Props = $props()

  function labelForStep(thoughtStep: SessionChatThoughtStep | null): string {
    if (!thoughtStep) return 'Working'
    if (thoughtStep.kind === 'thinking') return 'Thinking'
    return thoughtStep.title || (thoughtStep.kind === 'tool_call' ? 'Tool call' : 'Tool result')
  }

  function statusForStep(thoughtStep: SessionChatThoughtStep | null): string | null {
    return thoughtStep?.status ?? (active ? 'working' : null)
  }

  const status = $derived(statusForStep(step))
</script>

<button
  type="button"
  class={cn(
    'not-prose w-full rounded-xl border border-border/70 bg-muted/20 px-3 py-2.5 text-left shadow-sm transition hover:border-border hover:bg-muted/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
    className,
  )}
  aria-label="View thought details"
  onclick={onOpen}
>
  <div class="flex min-w-0 items-start">
    <span class="min-w-0 flex-1">
      <span class="flex min-w-0 items-center gap-2">
        <span class="truncate text-sm font-medium leading-5 text-foreground">{labelForStep(step)}</span>
        {#if status}<Badge variant="secondary" class="shrink-0 text-sm font-medium">{status}</Badge>{/if}
        {#if active}<span class="size-2 shrink-0 animate-pulse rounded-full bg-primary" aria-label="Working"></span>{/if}
        {#if stepCount > 1}<span class="ml-auto shrink-0 text-xs text-muted-foreground">{stepCount} steps</span>{/if}
      </span>
      <span class="mt-1 line-clamp-2 whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
        {#if step?.content}{step.content}{:else}Working…{/if}
      </span>
    </span>
  </div>
</button>
