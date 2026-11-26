<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { cn } from '$lib/utils'

  export type StageState = 'complete' | 'action' | 'unknown' | 'pending' | 'busy'
  let { stages, label }: { stages: Array<{ label: string; state: StageState }>; label: string } = $props()
</script>

<ol class="grid min-w-0 grid-cols-3 text-xs" aria-label={label}>
  {#each stages as stage, index (stage.label)}
    <li class={cn('flex min-w-0 items-start', stage.state === 'action' && 'font-medium text-warning', stage.state === 'busy' && 'text-primary', (stage.state === 'unknown' || stage.state === 'pending') && 'text-muted-foreground')}>
      <span class={cn('flex size-3.5 items-center justify-center rounded-full border text-10 leading-none', stage.state === 'complete' && 'border-success bg-success text-white', stage.state === 'action' && 'border-warning bg-warning text-white', stage.state === 'busy' && 'border-primary', (stage.state === 'unknown' || stage.state === 'pending') && 'border-muted-foreground/60')} aria-hidden="true">
        {#if stage.state === 'complete'}<CheckIcon class="size-2.5" weight="bold" />
        {:else if stage.state === 'action'}!
        {:else if stage.state === 'unknown'}?
        {:else if stage.state === 'busy'}<CircleNotchIcon class="size-2.5 animate-spin" />{/if}
      </span>
      <span class="min-w-0 pl-1.5 leading-3.5">{stage.label}</span>
      {#if index < stages.length - 1}<span class="mx-1 mt-[0.40625rem] h-px min-w-1 flex-1 bg-border sm:mx-2" aria-hidden="true"></span>{/if}
    </li>
  {/each}
</ol>
