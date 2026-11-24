<script lang="ts">
  import * as Sidebar from '../../../src/lib/components/ui/sidebar/index.js'
  import AppSidebar from '../../../src/components/layout/AppSidebar.svelte'
</script>

<Sidebar.Provider>
  <AppSidebar />
</Sidebar.Provider>
