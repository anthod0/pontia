<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import { cn } from '$lib/utils'
  import type { Snippet } from 'svelte'

  export type OnboardingStep = {
    label: string
    state: 'complete' | 'current' | 'pending'
    summary?: string
    content?: Snippet
  }
  let { steps, label }: { steps: OnboardingStep[]; label: string } = $props()
</script>

<ol class="space-y-0" aria-label={label}>
  {#each steps as step, index (step.label)}
    <li class="relative grid grid-cols-[28px_minmax(0,1fr)] gap-x-3 pb-5 last:pb-0">
      {#if index < steps.length - 1}<span class="absolute top-7 bottom-0 left-[13px] w-px bg-border" aria-hidden="true"></span>{/if}
      <span class={cn('relative z-10 flex size-7 items-center justify-center rounded-full border bg-popover text-xs font-semibold', step.state === 'complete' && 'border-success bg-success text-white', step.state === 'current' && 'border-primary text-primary', step.state === 'pending' && 'border-muted-foreground/50 text-muted-foreground')} aria-hidden="true">
        {#if step.state === 'complete'}<CheckIcon class="size-4" weight="bold" />{:else}{index + 1}{/if}
      </span>
      <div class="min-w-0 pt-1">
        <div class="flex flex-wrap items-baseline justify-between gap-2">
          <h3 class={cn('font-medium text-heading', step.state === 'pending' && 'text-muted-foreground')}>{step.label}</h3>
          {#if step.summary}<span class="text-xs text-success">{step.summary}</span>{/if}
        </div>
        {#if step.content}<div class="mt-2 rounded-lg border bg-muted/35 p-3">{@render step.content()}</div>{/if}
      </div>
    </li>
  {/each}
</ol>
