<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import type { SessionMetadataItem } from '../../../components/chat/sessionMetadata'
  let { items, refresh }: { items: SessionMetadataItem[]; refresh: () => void } = $props()
  let open = $state(false)
</script>

<div class="flex touch-pan-x gap-1.5 overflow-x-auto py-1">
  {#each items as item (item.key)}<button
      class="mobile-glass max-w-48 shrink-0 truncate rounded-full px-3 py-1 font-mono text-xs"
      aria-label={item.label + ': ' + item.value}
      onclick={() => {
        open = true
        refresh()
      }}>{item.value}</button
    >{/each}
</div>
<Sheet.Root bind:open
  ><BottomSheet dismiss={() => (open = false)} aria-describedby={undefined}
    ><Sheet.Title class="py-3 font-semibold">{$t('chat.details')}</Sheet.Title>
    <dl>
      {#each items as item (item.key)}<div class="flex min-h-11 items-center justify-between gap-3 border-t">
          <dt>{item.label}</dt>
          <dd class="min-w-0 break-all text-right font-mono text-xs">{item.title}</dd>
        </div>{/each}
    </dl></BottomSheet
  ></Sheet.Root
>
