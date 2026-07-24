<script lang="ts">
  import { CircleDot, XCircle } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'
  import BlocksWaveSpinner from './BlocksWaveSpinner.svelte'

  interface Props {
    state: string | null
    class?: string
  }

  let { state, class: className }: Props = $props()

  const status = $derived(agentStatusForState(state))

  function agentStatusForState(value: string | null): { label: string; tone: 'active' | 'error' | 'idle' | 'unknown' } | null {
    if (!value) return null
    switch (value) {
      case 'created': return { label: 'Session created', tone: 'idle' }
      case 'busy': return { label: 'Agent working', tone: 'active' }
      case 'idle': return null
      case 'exited': return null
      case 'error': return { label: 'Session error', tone: 'error' }
      default: return { label: `Agent status: ${value}`, tone: 'unknown' }
    }
  }
</script>

{#if status}
  <div
    class={cn('not-prose flex min-w-0 items-center gap-2 rounded-xl pr-3 pt-2.5 text-muted-foreground', className)}
    aria-label={`Agent status: ${status.label}`}
    role="status"
  >
    <span class="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground" aria-hidden="true">
      {#if state === 'busy'}
        <BlocksWaveSpinner class="size-5" />
      {:else if status.tone === 'error'}
        <XCircle class="size-4 text-destructive" />
      {:else}
        <CircleDot class="size-4" />
      {/if}
    </span>
    <span class="truncate text-xs leading-4">{status.label}</span>
  </div>
{/if}
