<script lang="ts">
  import { PanelLeft } from '@lucide/svelte'
  import * as Sidebar from '$lib/components/ui/sidebar/index.js'
</script>

<header class="sticky top-0 z-10 flex h-10 items-center bg-transparent px-3 md:px-4">
  <Sidebar.Trigger>
    <PanelLeft />
    <span class="sr-only">Toggle sidebar</span>
  </Sidebar.Trigger>
</header>
