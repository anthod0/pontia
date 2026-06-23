<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { get } from 'svelte/store'
  import { Router } from 'svelte-mini-router'
  import AuthGate from './components/auth/AuthGate.svelte'
  import AppShell from './components/layout/AppShell.svelte'
  import { Toaster } from './lib/components/ui/sonner/index.js'
  import { startEventStream, stopEventStream } from './services/eventStream'
  import { loadAgentProfiles } from './stores/agentProfiles'
  import { token } from './stores/auth'
  import { loadSessions } from './stores/sessions'
  import { loadTasks } from './stores/tasks'
  import { loadWorkspaces } from './stores/workspaces'
  import { routerConf } from './routes'

  let unsubscribeToken: (() => void) | null = null
  let dashboardStarted = false
  let authenticatedToken = get(token).trim()

  function startDashboard(): void {
    void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles(), loadSessions()])
    startEventStream()
    dashboardStarted = true
  }

  onMount(() => {
    unsubscribeToken = token.subscribe((value) => {
      const trimmed = value.trim()
      const previousToken = authenticatedToken
      authenticatedToken = trimmed
      if (!trimmed) {
        if (dashboardStarted) {
          stopEventStream()
          dashboardStarted = false
        }
        return
      }

      if (!dashboardStarted) {
        startDashboard()
        return
      }

      if (trimmed !== previousToken) {
        stopEventStream()
        startEventStream()
      }
    })
  })

  onDestroy(() => {
    unsubscribeToken?.()
    stopEventStream()
  })
</script>

<Toaster richColors />

{#if authenticatedToken}
  <AppShell>
    <Router {routerConf} />
  </AppShell>
{:else}
  <AuthGate />
{/if}
