<script lang="ts" module>
  import { cn, type WithElementRef } from '$lib/utils.js'
  import { CheckCircle2, Circle, CircleAlert, Wrench } from '@lucide/svelte'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface ChainOfThoughtStepProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    children?: Snippet
    title: string
    status?: string | null
    kind?: 'thinking' | 'tool_call' | 'tool_result'
  }
</script>

<script lang="ts">
  let { class: className, children, ref = $bindable(null), title, status = null, kind = 'thinking', ...restProps }: ChainOfThoughtStepProps = $props()
</script>

<div bind:this={ref} class={cn('space-y-1 rounded-md bg-muted/30 px-3 py-2', className)} {...restProps}>
  <div class="flex min-w-0 items-center gap-2 text-xs font-medium text-muted-foreground">
    {#if status === 'error'}
      <CircleAlert class="size-3.5 text-destructive" />
    {:else if kind === 'tool_call'}
      <Wrench class="size-3.5" />
    {:else if status === 'completed'}
      <CheckCircle2 class="size-3.5" />
    {:else}
      <Circle class="size-3.5" />
    {/if}
    <span class="min-w-0 truncate">{title}</span>
    {#if status}<span class="shrink-0 text-[0.7rem] uppercase tracking-wide opacity-70">{status}</span>{/if}
  </div>
  <div class="whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
    {@render children?.()}
  </div>
</div>
