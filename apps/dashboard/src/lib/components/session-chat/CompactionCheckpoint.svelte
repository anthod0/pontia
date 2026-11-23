<script lang="ts">
  import { t, currentLocale } from '$lib/i18n';
  import { Separator } from '$lib/components/ui/separator/index.js'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import { compactionCompletedLabel, type CompactionCheckpointItem } from '$lib/session-chat/compaction'

  let { item = null, busy = false }: { item?: CompactionCheckpointItem | null; busy?: boolean } = $props()
</script>

<div data-context-compaction={item?.item_id ?? 'pending'} role="status" aria-live="polite" class="py-2 text-xs text-muted-foreground">
  <div class="flex items-center gap-3">
    <Separator class="min-w-4 flex-1" />
    <span class="flex shrink-0 items-center gap-2">
      {#if busy}<SpinnerGapIcon class="size-3 animate-spin" aria-hidden="true" />{/if}
      {busy ? $t('compaction.running') : compactionCompletedLabel(item?.compaction_tokens, $t)}
    </span>
    {#if item?.occurred_at}<time datetime={item.occurred_at} title={new Date(item.occurred_at).toLocaleString($currentLocale)} class="shrink-0 tabular-nums">{new Date(item.occurred_at).toLocaleTimeString($currentLocale, { hour: '2-digit', minute: '2-digit' })}</time>{/if}
    <Separator class="min-w-4 flex-1" />
  </div>
</div>
