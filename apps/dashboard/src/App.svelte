<script lang="ts">
  import { Router } from 'svelte-mini-router'
  import AppShell from './components/layout/AppShell.svelte'
  import { routerConf } from './routes'
</script>

<AppShell>
  <Router {routerConf} />
</AppShell>
