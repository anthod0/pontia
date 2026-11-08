<script lang="ts">
  import { Separator } from '$lib/components/ui/separator/index.js'
  import type { TimelineItem } from '../../../api/types'

  let { item }: { item: TimelineItem } = $props()
</script>

<div data-context-compaction={item.item_id} class="py-2 text-xs text-muted-foreground">
  <div class="flex items-center gap-3">
    <Separator class="min-w-4 flex-1" />
    <span class="shrink-0">Context compacted</span>
    {#if item.occurred_at}<time datetime={item.occurred_at} title={new Date(item.occurred_at).toLocaleString()} class="shrink-0 tabular-nums">{new Date(item.occurred_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>{/if}
    <Separator class="min-w-4 flex-1" />
  </div>
</div>
