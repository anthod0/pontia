<script lang="ts">
  import { ChevronDown, CircleDot, Hammer, Sparkles } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { SessionChatThoughtStep } from '../../session-chat/sessionChat'

  interface Props {
    steps: SessionChatThoughtStep[]
    active?: boolean
    class?: string
  }

  let { steps, active = false, class: className }: Props = $props()
  let open = $state(false)
  const latestStep = $derived(steps.at(-1) ?? null)
  const visibleSteps = $derived(open ? steps : latestStep ? [latestStep] : [])

  function labelForStep(step: SessionChatThoughtStep): string {
    if (step.kind === 'thinking') return 'Thinking'
    if (step.kind === 'tool_call') return 'Tool call'
    return 'Tool result'
  }

  function iconForStep(step: SessionChatThoughtStep | null): typeof Sparkles {
    if (!step || step.kind === 'thinking') return Sparkles
    if (step.kind === 'tool_call') return Hammer
    return CircleDot
  }
</script>

<section class={cn('not-prose rounded-xl border border-border/70 bg-muted/25 p-3 text-sm shadow-sm', className)} aria-label="Thought summary">
  <div class="flex min-w-0 items-start gap-3">
    <div class="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full bg-background text-muted-foreground">
      {#if latestStep}
        {@const Icon = iconForStep(latestStep)}
        <Icon class="size-3.5" />
      {:else}
        <Sparkles class="size-3.5" />
      {/if}
    </div>

    <div class="min-w-0 flex-1 space-y-2">
      <div class="flex min-w-0 items-center gap-2">
        <span class="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {#if latestStep}{labelForStep(latestStep)}{:else}Working{/if}
        </span>
        {#if active}
          <span class="flex items-center gap-1 text-xs text-muted-foreground"><span class="size-1.5 animate-pulse rounded-full bg-primary"></span>Working…</span>
        {/if}
        {#if steps.length > 1}
          <Badge variant="secondary" class="ml-auto shrink-0">{steps.length} steps</Badge>
        {/if}
      </div>

      {#if visibleSteps.length}
        <div class="space-y-2">
          {#each visibleSteps as step (step.id)}
            <article class="min-w-0 rounded-lg bg-background/70 px-3 py-2">
              <div class="mb-1 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                <span class="font-medium text-foreground">{step.title || labelForStep(step)}</span>
                {#if step.status}<span>· {step.status}</span>{/if}
              </div>
              <p class="line-clamp-3 whitespace-pre-wrap break-words text-sm leading-5 text-foreground/90">{step.content}</p>
            </article>
          {/each}
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">Working…</p>
      {/if}
    </div>

    {#if steps.length > 1}
      <button
        type="button"
        class="mt-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition hover:bg-background hover:text-foreground"
        aria-label={open ? 'Collapse thought summaries' : 'Show all thought summaries'}
        aria-expanded={open}
        onclick={() => (open = !open)}
      >
        <ChevronDown class={cn('size-4 transition-transform', open ? 'rotate-180' : '')} />
      </button>
    {/if}
  </div>
</section>
