<script lang="ts">
  import { onMount } from 'svelte'
  import { toast } from 'svelte-sonner'
  import { t } from '$lib/i18n'
  import { dashboardRelativePath, navigate } from '$lib/navigation'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentClientAuthStatus, AgentClientInstallMethod, AgentClientView } from '../../api/types'
  import AgentClientOnboardingDialog from './AgentClientOnboardingDialog.svelte'
  import { useAgentClientOnboardingStatus } from './context.svelte'
  import {
    authForClient,
    runAgentClientOnboardingProbe,
    shouldShowAgentClientOnboarding,
  } from './agentClientOnboarding'

  const device = useDeviceRuntime()
  const status = useAgentClientOnboardingStatus()
  let open = $state(false)
  let clients = $state<AgentClientView[]>([])
  let initialAuth = $state<Array<{ client: AgentClientView; auth: AgentClientAuthStatus }>>([])

  function delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms))
  }

  function inputFocused(): boolean {
    return document.activeElement?.matches('input, textarea, [contenteditable="true"]') ?? false
  }

  function anotherModalOpen(): boolean {
    return document.querySelector('[data-slot="dialog-content"][data-state="open"], [data-slot="sheet-content"][data-state="open"], [data-slot="alert-dialog-content"][data-state="open"]') !== null
  }

  async function waitUntilReady(isActive: () => boolean): Promise<boolean> {
    if (dashboardRelativePath() === '/settings/clients') return false
    while (isActive() && (inputFocused() || anotherModalOpen())) {
      await delay(150)
      if (dashboardRelativePath() === '/settings/clients') return false
    }
    return isActive()
  }

  onMount(() => {
    let active = true
    status!.probing = localStorage.getItem(`pontia.agentClientOnboarding.checked.${device.id}`) === null
    void runAgentClientOnboardingProbe({
      deviceId: device.id,
      storage: localStorage,
      probe: async () => {
        const [listed, probe] = await Promise.all([
          device.api.listAgentClients(),
          device.api.probeAgentClientAuth(),
        ])
        return { listed, probe }
      },
    }).then(async ({ result }) => {
      if (!active || !result || !shouldShowAgentClientOnboarding(result.listed, result.probe)) return
      if (!(await waitUntilReady(() => active))) return
      clients = result.listed
      initialAuth = result.listed.flatMap(client => {
        const auth = authForClient(client, result.probe)
        return auth ? [{ client, auth }] : []
      })
      open = true
    }).catch(() => {
      // A failed first probe is intentionally final for this browser and device.
    }).finally(() => {
      status!.probing = false
    })
    return () => { active = false; status!.probing = false }
  })

  async function install(client: AgentClientView, method: AgentClientInstallMethod): Promise<AgentClientView> {
    let operation = await device.api.installAgentClient(client, method.id)
    while (operation.status === 'running') {
      await delay(750)
      operation = await device.api.getAgentClientOperation(operation.id, operation.next_offset)
    }
    if (operation.status !== 'succeeded') throw new Error(operation.error ?? $t('onboarding.clients.install.failed'))
    const listed = await device.api.listAgentClients()
    clients = listed
    const updated = listed.find(entry => entry.id === client.id)
    if (!updated) throw new Error($t('onboarding.clients.install.failed'))
    return updated
  }

  async function enable(client: AgentClientView): Promise<AgentClientView> {
    let current = client
    if (current.setup.status === 'required') {
      let operation = await device.api.setupAgentClient(current)
      while (operation.status === 'running') {
        await delay(750)
        operation = await device.api.getAgentClientSetup() ?? operation
      }
      if (operation.status !== 'succeeded') throw new Error(operation.error ?? $t('onboarding.clients.enable.failed'))
      current = (await device.api.listAgentClients()).find(entry => entry.id === current.id) ?? current
    }
    if (!current.enabled) current = await device.api.patchAgentClient(current, { enabled: true })
    clients = clients.map(entry => entry.id === current.id ? current : entry)
    await Promise.all([
      device.queryClient.invalidateQueries({ queryKey: [device.id, 'session-creation-options'] }),
      device.queryClient.invalidateQueries({ queryKey: ['bot-clients'] }),
    ])
    return current
  }

  function skip() {
    if (isMobileShell()) {
      toast.info($t('onboarding.clients.toast.skipped'))
      return
    }
    toast.info($t('onboarding.clients.toast.skipped'), {
      action: { label: $t('onboarding.clients.toast.goSettings'), onClick: () => void navigate('/settings/clients') },
    })
  }

  function manage() {
    open = false
    void navigate('/settings/clients')
  }

  function complete() {
    void device.queryClient.invalidateQueries({
      predicate: query => {
        const key = query.queryKey
        return key[0] === 'bot-clients' || key[0] === 'agent-client-models' || (key[0] === device.id && key[1] === 'session-creation-options')
      },
    })
    if (isMobileShell()) void navigate('/bot')
    else document.querySelector<HTMLElement>('textarea, [contenteditable="true"]')?.focus()
  }
</script>

{#if clients.length}
  <AgentClientOnboardingDialog bind:open deviceName={device.name ?? $t('onboarding.clients.thisDevice')} {clients} {initialAuth}
    {install} getAuth={device.api.getAgentClientAuth} startLogin={device.api.startAgentClientLogin}
    getLogin={device.api.getAgentClientLoginSession} streamLogin={device.api.streamAgentClientLoginSession}
    cancelLogin={device.api.cancelAgentClientLogin} submitLoginInput={device.api.submitAgentClientLoginInput}
    {enable} onSkip={skip} onManage={manage} onComplete={complete} />
{/if}
