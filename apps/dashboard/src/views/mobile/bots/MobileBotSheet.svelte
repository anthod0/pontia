<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  let { open = $bindable(false), title, subtitle, fixed = false, children }: { open: boolean; title?: string; subtitle?: string; fixed?: boolean; children: Snippet } = $props()
</script>

<Sheet.Root bind:open>
  <BottomSheet
    data-mobile-bot-sheet
    class={fixed ? 'data-[side=bottom]:h-[92dvh] px-0' : undefined}
    onInteractOutside={(event) => {
      if (event.target instanceof Element && event.target.closest('[data-sonner-toast]')) event.preventDefault()
    }}
    dismiss={() => (open = false)}
    aria-describedby={undefined}
  >
    {#if title}<div class={['flex shrink-0 items-center gap-2 py-3 pr-14', fixed ? 'border-b px-4' : 'px-0']}><Sheet.Title class="text-lg font-semibold">{title}</Sheet.Title>{#if subtitle}<span class="truncate text-[13px] text-muted-foreground">{subtitle}</span>{/if}</div>{/if}
    <div class={['min-h-0 overflow-auto overscroll-contain pb-3', fixed && 'flex flex-1 flex-col pb-0']}>{@render children()}</div>
  </BottomSheet>
</Sheet.Root>
