<script lang="ts">
  import type { Snippet } from 'svelte'
  import DesktopShell from './desktop/DesktopShell.svelte'

  let { children }: { children: Snippet } = $props()
</script>

<DesktopShell>{@render children()}</DesktopShell>
