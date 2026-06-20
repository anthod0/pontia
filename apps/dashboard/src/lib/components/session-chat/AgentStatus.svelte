<script lang="ts">
  import { CheckCircle2, CircleDot, XCircle } from '@lucide/svelte'
  import { cn } from '$lib/utils.js'
  import BlocksWaveSpinner from './BlocksWaveSpinner.svelte'

  interface Props {
    state: string | null
    class?: string
  }

  let { state, class: className }: Props = $props()

  const status = $derived(agentStatusForState(state))

  function agentStatusForState(value: string | null): { label: string; tone: 'active' | 'success' | 'error' | 'idle' | 'unknown' } | null {
    if (!value) return null
    switch (value) {
      case 'created': return { label: 'Session created', tone: 'idle' }
      case 'starting': return { label: 'Session starting', tone: 'active' }
      case 'busy': return { label: 'Agent working', tone: 'active' }
      case 'idle': return null
      case 'exited': return { label: 'Session exited', tone: 'success' }
      case 'error': return { label: 'Session error', tone: 'error' }
      default: return { label: `Agent status: ${value}`, tone: 'unknown' }
    }
  }
</script>

{#if status}
  <div
    class={cn('not-prose flex min-w-0 items-center gap-2 rounded-xl px-3 pt-2.5 text-muted-foreground', className)}
    aria-label={`Agent status: ${status.label}`}
    role="status"
  >
    <span class="inline-flex size-5 shrink-0 items-center justify-center text-muted-foreground" aria-hidden="true">
      {#if state === 'busy' || state === 'starting'}
        <BlocksWaveSpinner class="size-5" />
      {:else if status.tone === 'success'}
        <CheckCircle2 class="size-4" />
      {:else if status.tone === 'error'}
        <XCircle class="size-4 text-destructive" />
      {:else}
        <CircleDot class="size-4" />
      {/if}
    </span>
    <span class="truncate text-xs leading-4">{status.label}</span>
  </div>
{/if}
