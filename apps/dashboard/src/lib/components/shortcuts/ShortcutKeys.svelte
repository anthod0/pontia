<script lang="ts">
  import * as Kbd from '$lib/components/ui/kbd/index.js'
  import type { ShortcutId } from '$lib/shortcuts/registry'
  import { shortcuts } from '$lib/shortcuts/store.svelte'
  import { formatShortcutKeys } from '$lib/shortcuts/format'

  let { keys, shortcutId, class: className, ...restProps }: { keys?: string[]; shortcutId?: ShortcutId; class?: string; 'aria-hidden'?: boolean | 'true' | 'false' } = $props()
  const label = $derived(shortcutId ? shortcuts.label(shortcutId) : formatShortcutKeys(keys ?? []))
</script>

{#if label}<Kbd.Root class={className} {...restProps}>{label}</Kbd.Root>{/if}
