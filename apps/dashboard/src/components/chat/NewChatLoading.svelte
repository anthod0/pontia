<script lang="ts">
  import type { Snippet } from 'svelte'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'

  let { message, deviceSelector }: { message: string; deviceSelector?: Snippet<[boolean]> } = $props()
</script>

<div class="mx-auto w-full max-w-[760px] space-y-3 pb-[max(0.5rem,env(safe-area-inset-bottom))] md:pb-[max(1.25rem,env(safe-area-inset-bottom))]">
  <div class="flex min-w-0 items-center gap-2">
    {@render deviceSelector?.(false)}
    <Skeleton class="h-[34px] w-28" />
    <Skeleton class="h-[34px] w-32" />
  </div>
  <div class="flex min-h-32 items-center justify-center gap-2 rounded-xl border border-border bg-background p-4 text-sm text-muted-foreground" role="status">
    <SpinnerGapIcon class="size-4 shrink-0 animate-spin" aria-hidden="true" />
    <span>{message}</span>
  </div>
</div>
