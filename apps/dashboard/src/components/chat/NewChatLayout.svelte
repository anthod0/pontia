<script lang="ts">
  import type { Snippet } from 'svelte'
  import PontiaMascot from './PontiaMascot.svelte'

  type Status = { mood: 'loading' | 'offline'; title: string; description: string }

  let { status, action, children }: { status?: Status; action?: Snippet; children: Snippet } = $props()
</script>

<section class="flex min-h-[calc(100svh-3.25rem)] flex-col gap-6">
  <div class="mx-auto flex w-full max-w-[720px] flex-1 items-center justify-center py-12 text-center">
    {#if status}
      <div>
        <div role={status.mood === 'loading' ? 'status' : 'alert'}>
          <PontiaMascot mood={status.mood} class="mx-auto mb-4 size-20" />
          <h1 class="mb-2 text-[28px] font-semibold tracking-tight text-heading">{status.title}</h1>
          <p class="text-sm text-foreground">{status.description}</p>
        </div>
        {#if action}<div class="mt-5">{@render action()}</div>{/if}
      </div>
    {:else}
      <div>
        <PontiaMascot class="mx-auto mb-4 size-20" />
        <h1 class="mb-2 text-[28px] font-semibold tracking-tight text-heading">Start a session</h1>
        <p class="text-sm text-foreground">Choose a workspace and agent client, then describe the job.</p>
      </div>
    {/if}
  </div>
  {@render children()}
</section>
