<script lang="ts">
  import type { Snippet } from 'svelte'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  let { open = $bindable(false), title, children }: { open: boolean; title: string; children: Snippet } = $props()
</script>

<Sheet.Root bind:open>
  <BottomSheet
    data-mobile-bot-sheet
    onInteractOutside={(event) => {
      if (event.target instanceof Element && event.target.closest('[data-sonner-toast]')) event.preventDefault()
    }}
    dismiss={() => (open = false)}
    aria-describedby={undefined}
  >
    <Sheet.Title class="py-3 pr-12 text-lg font-semibold">{title}</Sheet.Title>
    <div class="min-h-0 overflow-auto overscroll-contain pb-3">{@render children()}</div>
  </BottomSheet>
</Sheet.Root>
