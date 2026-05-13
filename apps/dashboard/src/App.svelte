<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Router } from 'svelte-mini-router'
  import AppShell from './components/layout/AppShell.svelte'
  import { startEventStream, stopEventStream } from './services/eventStream'
  import { loadAgentProfiles } from './stores/agentProfiles'
  import { token } from './stores/auth'
  import { loadTasks } from './stores/tasks'
  import { loadWorkspaces } from './stores/workspaces'
  import { routerConf } from './routes'

  let unsubscribeToken: (() => void) | null = null

  onMount(() => {
    void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles()])
    startEventStream()
    unsubscribeToken = token.subscribe(() => {
      stopEventStream()
      startEventStream()
    })
  })

  onDestroy(() => {
    unsubscribeToken?.()
    stopEventStream()
  })
</script>

<AppShell>
  <Router {routerConf} />
</AppShell>
