<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { cn } from '$lib/utils'

  export type StageState = 'complete' | 'action' | 'unknown' | 'pending' | 'busy'
  let { stages, label }: { stages: Array<{ label: string; state: StageState }>; label: string } = $props()
</script>

<ol class="flex min-w-0 items-center text-xs" aria-label={label}>
  {#each stages as stage, index (stage.label)}
    {#if index > 0}<span class="mx-2 h-px min-w-2 flex-1 bg-border sm:mx-3.5" aria-hidden="true"></span>{/if}
    <li class={cn('flex shrink-0 items-center gap-1.5 whitespace-nowrap', stage.state === 'action' && 'font-medium text-warning', stage.state === 'busy' && 'text-primary', (stage.state === 'unknown' || stage.state === 'pending') && 'text-muted-foreground')}>
      <span class={cn('flex size-3.5 items-center justify-center rounded-full border text-[9px] leading-none', stage.state === 'complete' && 'border-success bg-success text-white', stage.state === 'action' && 'border-warning bg-warning text-white', stage.state === 'busy' && 'border-primary', (stage.state === 'unknown' || stage.state === 'pending') && 'border-muted-foreground/60')} aria-hidden="true">
        {#if stage.state === 'complete'}<CheckIcon class="size-2.5" weight="bold" />
        {:else if stage.state === 'action'}!
        {:else if stage.state === 'unknown'}?
        {:else if stage.state === 'busy'}<CircleNotchIcon class="size-2.5 animate-spin" />{/if}
      </span>
      <span>{stage.label}</span>
    </li>
  {/each}
</ol>
