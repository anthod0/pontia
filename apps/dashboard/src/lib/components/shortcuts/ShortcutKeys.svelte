<script lang="ts">
  import * as Kbd from '$lib/components/ui/kbd/index.js'
  import type { ShortcutKey } from '$lib/shortcuts/definitions'
  import { formatShortcutKeys } from '$lib/shortcuts/format'

  let { keys, class: className, ...restProps }: { keys: ShortcutKey[]; class?: string; 'aria-hidden'?: boolean | 'true' | 'false' } = $props()
</script>

<Kbd.Root class={className} {...restProps}>{formatShortcutKeys(keys)}</Kbd.Root>
