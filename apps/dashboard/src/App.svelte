<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Router } from 'svelte-mini-router'
  import AuthGate from './components/auth/AuthGate.svelte'
  import AppShell from './components/layout/AppShell.svelte'
  import { Toaster } from './lib/components/ui/sonner/index.js'
  import { validateExternalApiToken } from './api/client'
  import { startEventStream, stopEventStream } from './services/eventStream'
  import { loadAgentProfiles } from './stores/agentProfiles'
  import { token } from './stores/auth'
  import { loadSessions } from './stores/sessions'
  import { loadTasks } from './stores/tasks'
  import { loadWorkspaces } from './stores/workspaces'
  import { routerConf } from './routes'

  let unsubscribeToken: (() => void) | null = null
  let dashboardStarted = false
  let authenticatedToken = ''
  let validationRun = 0

  function startDashboard(): void {
    void Promise.all([loadTasks(), loadWorkspaces(), loadAgentProfiles(), loadSessions()])
    startEventStream()
    dashboardStarted = true
  }

  onMount(() => {
    unsubscribeToken = token.subscribe((value) => {
      const trimmed = value.trim()
      const run = ++validationRun
      if (!trimmed) {
        authenticatedToken = ''
        if (dashboardStarted) {
          stopEventStream()
          dashboardStarted = false
        }
        return
      }

      validateExternalApiToken(trimmed).then(() => {
        if (run !== validationRun) return
        authenticatedToken = trimmed
        if (!dashboardStarted) {
          startDashboard()
          return
        }

        stopEventStream()
        startEventStream()
      }).catch(() => {
        if (run !== validationRun) return
        authenticatedToken = ''
        token.set('')
      })
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
