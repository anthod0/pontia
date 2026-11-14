<script lang="ts">
  import type { Snippet } from 'svelte'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'

  let { deviceSelector }: { deviceSelector?: Snippet<[boolean]> } = $props()
</script>

<div class="mx-auto w-full max-w-[760px] space-y-3 pb-[max(0.5rem,env(safe-area-inset-bottom))] md:pb-[max(1.25rem,env(safe-area-inset-bottom))]">
  <div class="flex min-w-0 items-center gap-2">
    {@render deviceSelector?.(false)}
    <Skeleton class="h-[34px] w-28" />
    <Skeleton class="h-[34px] w-32" />
  </div>
  <!-- Decorative placeholder matching the compact composer; the page heading announces the loading state. -->
  <div class="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center rounded-lg border bg-background" aria-hidden="true" inert>
    <div class="py-1 pl-1">
      <span class="flex size-8 items-center justify-center text-muted-foreground opacity-50"><PlusIcon class="size-4" /></span>
    </div>
    <div class="py-2 text-base text-muted-foreground sm:text-[13.5px]">Describe a task…</div>
    <div class="p-1">
      <span class="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-semibold text-primary-foreground opacity-50 max-sm:w-7 max-sm:justify-center max-sm:px-0">
        <span class="max-sm:sr-only">Start session</span>
        <ArrowUpIcon class="size-3.5" />
      </span>
    </div>
  </div>
</div>
