<script lang="ts" module>
  export type DeviceStatusView = { mood: 'loading' | 'offline'; title: string; description: string }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte'
  import PontiaMascot from '../chat/PontiaMascot.svelte'

  let { status, action }: { status: DeviceStatusView; action?: Snippet } = $props()
</script>

<div class="text-center">
  <div role={status.mood === 'loading' ? 'status' : 'alert'}>
    <PontiaMascot mood={status.mood} class="mx-auto mb-4 size-20" />
    <h1 class="mb-2 text-[28px] font-semibold tracking-tight text-heading">{status.title}</h1>
    <p class="text-sm text-foreground">{status.description}</p>
  </div>
  {#if action}<div class="mt-5">{@render action()}</div>{/if}
</div>
