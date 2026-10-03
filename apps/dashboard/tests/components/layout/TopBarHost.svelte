<script lang="ts">
  import * as Sidebar from '../../../src/lib/components/ui/sidebar/index.js'
  import TopBar from '../../../src/components/layout/TopBar.svelte'

  let { onOpenChange }: { onOpenChange?: (open: boolean) => void } = $props()
</script>

<Sidebar.Provider {onOpenChange}>
  <TopBar />
</Sidebar.Provider>
