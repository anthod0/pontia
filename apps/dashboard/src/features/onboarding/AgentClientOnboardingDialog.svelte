<script lang="ts">
  import { t } from '$lib/i18n'
  import { copyText } from '$lib/copyText'
  import { cn } from '$lib/utils'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import * as RadioGroup from '$lib/components/ui/radio-group/index.js'
  import ResponsiveSelect from '$lib/components/ui/overlay/ResponsiveSelect.svelte'
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import CaretLeftIcon from 'phosphor-svelte/lib/CaretLeftIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import PowerIcon from 'phosphor-svelte/lib/PowerIcon'
  import QuestionIcon from 'phosphor-svelte/lib/QuestionIcon'
  import SparkleIcon from 'phosphor-svelte/lib/SparkleIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon'
  import ClientIcon from '../../components/chat/ClientIcon.svelte'
  import type {
    AgentClientAuthSession,
    AgentClientAuthField,
    AgentClientAuthStatus,
    AgentClientInstallMethod,
    AgentClientView,
  } from '../../api/types'
  import { onboardingReason } from './agentClientOnboarding'
  import ClientStageTrack, { type StageState } from './ClientStageTrack.svelte'
  import OnboardingSteps, { type OnboardingStep } from './OnboardingSteps.svelte'
  import { readSseFrames } from '../../services/readSse'
  import { IsMobile } from '$lib/hooks/is-mobile.svelte'

  interface Props {
    open?: boolean
    deviceName: string
    clients: AgentClientView[]
    initialAuth: Array<{ client: AgentClientView; auth: AgentClientAuthStatus }>
    install: (client: AgentClientView, method: AgentClientInstallMethod) => Promise<AgentClientView>
    getAuth: (client: AgentClientView) => Promise<AgentClientAuthStatus>
    startLogin: (client: AgentClientView, methodId: string, fields: Record<string, string>) => Promise<AgentClientAuthSession>
    getLogin: (client: AgentClientView, sessionId: string) => Promise<AgentClientAuthSession>
    streamLogin: (client: AgentClientView, sessionId: string, signal: AbortSignal) => Promise<Response>
    cancelLogin: (client: AgentClientView, sessionId: string) => Promise<AgentClientAuthSession>
    submitLoginInput: (client: AgentClientView, sessionId: string, input: unknown) => Promise<AgentClientAuthSession>
    enable: (client: AgentClientView) => Promise<AgentClientView>
    onSkip: () => void
    onManage: () => void
    onComplete: () => void
  }

  let {
    open = $bindable(true), deviceName, clients, initialAuth, install, getAuth, startLogin,
    getLogin, streamLogin, cancelLogin, submitLoginInput, enable, onSkip, onManage, onComplete,
  }: Props = $props()

  let view = $state<'pick' | 'flow' | 'done'>('pick')
  function initialClients(): AgentClientView[] {
    return [...clients]
  }

  function initialAuthById(): Record<string, AgentClientAuthStatus> {
    return Object.fromEntries(initialAuth.map(({ client, auth }) => [client.id, auth]))
  }

  let selectedId = $state<string | null>(null)
  let localClients = $state(initialClients())
  let authById = $state<Record<string, AgentClientAuthStatus>>(initialAuthById())
  let terminalConfirmed = $state<string[]>([])
  let installing = $state(false)
  let enabling = $state(false)
  let authLoading = $state(false)
  let loginSession = $state<AgentClientAuthSession | null>(null)
  let selectedMethodId = $state('')
  let loginFields = $state<Record<string, string>>({})
  let sessionFields = $state<Record<string, string>>({})
  let installExpanded = $state(false)
  let error = $state('')
  const mobile = new IsMobile()

  const selected = $derived(localClients.find(client => client.id === selectedId) ?? null)
  const selectedAuth = $derived(selected ? authById[selected.id] : undefined)
  const installed = $derived.by(() => localClients.filter(client => client.detection.status !== 'not_detected').map((client, index) => ({client, index})).sort((left, right) => {
    const rank = (client: AgentClientView) => {
      const state = authById[client.id]?.state
      const remaining = (state === 'authenticated' ? 0 : 1) + (client.enabled && client.setup.status !== 'required' ? 0 : 1)
      const loginRank = state === 'unauthenticated' || state === 'expired' ? 0 : 1
      return [remaining, loginRank] as const
    }
    const a = rank(left.client); const b = rank(right.client)
    return a[0] - b[0] || a[1] - b[1] || left.index - right.index
  }).map(entry => entry.client))
  const installable = $derived(localClients.filter(client => client.detection.status === 'not_detected' && client.install?.supported && client.install.methods.length))
  const authMap = $derived(new Map(Object.entries(authById)))
  const reason = $derived(onboardingReason(localClients, authMap))
  const signedIn = $derived(!!selected && (selectedAuth?.state === 'authenticated' || terminalConfirmed.includes(selected.id)))
  const activated = $derived(!!selected && selected.enabled && selected.setup.status !== 'required')
  const currentStep = $derived(!selected || selected.detection.status === 'not_detected' ? 0 : !signedIn ? 1 : !activated ? 2 : 3)
  const preferredMethod = $derived(selected?.install?.methods.find(method => method.recommended) ?? selected?.install?.methods[0] ?? null)
  const managedMethod = $derived(selectedAuth?.methods.find(method => method.id === selectedMethodId) ?? selectedAuth?.methods.find(method => method.recommended) ?? selectedAuth?.methods[0])
  const unknownNames = $derived(installed.filter(client => client.enabled && client.setup.status !== 'required' && ['unknown', 'unsupported'].includes(authById[client.id]?.state ?? '')).map(client => client.name).join(', '))
  const terminalCommand = $derived(selected?.install?.auth.headless_login_command ?? selected?.install?.auth.login_command)

  $effect(() => {
    if (view === 'flow' && selected && currentStep === 3) view = 'done'
  })

  const steps = $derived.by((): OnboardingStep[] => {
    if (!selected) return []
    return [
      {
        label: $t('onboarding.clients.stage.install'),
        state: currentStep > 0 ? 'complete' : 'current',
        summary: currentStep > 0 ? $t('onboarding.clients.install.done', { version: selected.version ?? '' }) : undefined,
        content: currentStep === 0 ? installContent : undefined,
      },
      {
        label: $t('onboarding.clients.stage.login'),
        state: currentStep > 1 ? 'complete' : currentStep === 1 ? 'current' : 'pending',
        summary: currentStep > 1 ? (terminalConfirmed.includes(selected.id) ? $t('onboarding.clients.login.signedInTerminalSum') : $t('onboarding.clients.login.signedIn')) : undefined,
        content: currentStep === 1 ? loginContent : undefined,
      },
      {
        label: selected.setup.status === 'required' ? $t('onboarding.clients.stage.enableSetup') : $t('onboarding.clients.stage.enable'),
        state: currentStep > 2 ? 'complete' : currentStep === 2 ? 'current' : 'pending',
        summary: currentStep > 2 ? $t('onboarding.clients.enable.done') : undefined,
        content: currentStep === 2 ? enableContent : undefined,
      },
    ]
  })

  function authState(client: AgentClientView): AgentClientAuthStatus | undefined {
    return authById[client.id]
  }

  function stages(client: AgentClientView): Array<{ label: string; state: StageState }> {
    const operationInstalled = client.detection.status !== 'not_detected'
    const auth = authState(client)
    const loginState: StageState = auth?.state === 'authenticated'
      ? 'complete'
      : auth?.state === 'unknown' || auth?.state === 'unsupported' || !auth
        ? 'unknown'
        : 'action'
    return [
      { label: $t('onboarding.clients.stage.install'), state: operationInstalled ? 'complete' : installing && selectedId === client.id ? 'busy' : 'pending' },
      { label: auth?.state === 'expired' ? $t('onboarding.clients.stage.expired') : auth?.state === 'unauthenticated' ? $t('onboarding.clients.stage.signedOut') : auth?.state === 'unknown' || auth?.state === 'unsupported' || !auth ? $t('onboarding.clients.stage.unknown') : $t('onboarding.clients.stage.login'), state: operationInstalled ? loginState : 'pending' },
      { label: client.setup.status === 'required' ? $t('onboarding.clients.stage.enableSetup') : $t('onboarding.clients.stage.enable'), state: client.enabled && client.setup.status !== 'required' ? 'complete' : 'pending' },
    ]
  }

  function replaceClient(client: AgentClientView) {
    localClients = localClients.map(entry => entry.id === client.id ? client : entry)
  }

  async function choose(client: AgentClientView) {
    selectedId = client.id
    error = ''
    loginSession = null
    view = 'flow'
    const known = authById[client.id]
    if (known) prepareLogin(known)
    else if (client.detection.status !== 'not_detected') await loadAuth(client)
  }

  function prepareLogin(auth: AgentClientAuthStatus) {
    const method = auth.methods.find(entry => entry.recommended) ?? auth.methods[0]
    selectedMethodId = method?.id ?? ''
    loginFields = Object.fromEntries((method?.start_fields ?? []).map(field => [field.name, field.default_value ?? '']))
  }

  function fieldOptions(field: AgentClientAuthField): Array<{ value: string; label: string }> {
    return field.kind.type === 'select' ? field.kind.options : []
  }

  function fieldOptionLabel(field: AgentClientAuthField, value: string): string {
    return fieldOptions(field).find(option => option.value === value)?.label ?? field.placeholder ?? field.label
  }

  async function loadAuth(client: AgentClientView) {
    authLoading = true
    error = ''
    try {
      const auth = await getAuth(client)
      authById[client.id] = auth
      if (selectedId === client.id) prepareLogin(auth)
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      authLoading = false
    }
  }

  async function runInstall() {
    const client = selected
    const method = preferredMethod
    if (!client || !method || installing) return
    installing = true
    error = ''
    try {
      const updated = await install(client, method)
      replaceClient(updated)
      if (selectedId === client.id) await loadAuth(updated)
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      installing = false
    }
  }

  async function runLogin() {
    const client = selected
    const method = managedMethod
    if (!client || !method || loginSession?.state === 'starting' || loginSession?.state === 'awaiting_user') return
    error = ''
    try {
      const fields = Object.fromEntries(method.start_fields.map(field => [field.name, loginFields[field.name] ?? '']))
      let session = await startLogin(client, method.id, fields)
      if (selectedId === client.id) loginSession = session
      if (!['succeeded', 'failed', 'cancelled', 'timed_out'].includes(session.state)) {
        const controller = new AbortController()
        const response = await streamLogin(client, session.session_id, controller.signal)
        await readSseFrames(response.body!, frame => {
          session = JSON.parse(frame.data) as AgentClientAuthSession
          if (session.step.type === 'await_input') sessionFields = Object.fromEntries(session.step.fields.map(field => [field.name, field.default_value ?? '']))
          if (selectedId === client.id) loginSession = session
        })
        if (!['succeeded', 'failed', 'cancelled', 'timed_out'].includes(session.state)) session = await getLogin(client, session.session_id)
      }
      if (session.state === 'succeeded' && session.step.type === 'succeeded') {
        authById[client.id] = session.step.status
      }
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }

  async function sendLoginInput() {
    const session = loginSession
    const client = localClients.find(entry => entry.client_type === session?.client_type && entry.client_key === session.client_key)
    if (!session || !client || session.step.type !== 'await_input') return
    loginSession = await submitLoginInput(client, session.session_id, { type: 'fields', values: sessionFields })
  }

  async function stopLogin() {
    const session = loginSession
    const client = localClients.find(entry => entry.client_type === session?.client_type && entry.client_key === session.client_key)
    if (!session || !client) return
    loginSession = await cancelLogin(client, session.session_id)
  }

  function confirmTerminal() {
    if (!selected || terminalConfirmed.includes(selected.id)) return
    terminalConfirmed = [...terminalConfirmed, selected.id]
  }

  async function runEnable() {
    const client = selected
    if (!client || enabling) return
    enabling = true
    error = ''
    try {
      const updated = await enable(client)
      replaceClient(updated)
      if (selectedId === client.id && view === 'flow') view = 'done'
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      enabling = false
    }
  }

  function dismiss() {
    open = false
    onSkip()
  }

  function reasonText(): string {
    return $t(`onboarding.clients.reason.${reason}`)
  }

  function rowAction(client: AgentClientView): string {
    if (client.detection.status === 'not_detected') return $t('onboarding.clients.action.install')
    const auth = authState(client)
    if (auth?.state === 'unauthenticated' || auth?.state === 'expired') return $t('onboarding.clients.action.login')
    if (auth?.state === 'authenticated' && (!client.enabled || client.setup.status === 'required')) return client.setup.status === 'required' ? $t('onboarding.clients.action.enableSetup') : $t('onboarding.clients.action.enable')
    return auth?.state === 'unknown' || auth?.state === 'unsupported' || !auth ? $t('onboarding.clients.action.view') : $t('onboarding.clients.action.continue')
  }
</script>

{#snippet installContent()}
  {#if preferredMethod}
    <p class="text-sm text-muted-foreground">{preferredMethod.label} · {preferredMethod.target_summary}</p>
    {#if !preferredMethod.ready}
      <ul class="mt-2 space-y-1 text-sm text-warning">
        {#each preferredMethod.prerequisites.filter(item => item.required && !item.satisfied) as item (item.id)}<li>{item.label}{item.missing_hint ? ` · ${item.missing_hint}` : ''}</li>{/each}
      </ul>
      <p class="mt-2 text-xs text-muted-foreground">{$t('onboarding.clients.install.blockedHint')}</p>
    {/if}
  {/if}
{/snippet}

{#snippet loginContent()}
  {#if authLoading}
    <p class="flex items-center gap-2 text-muted-foreground" role="status"><CircleNotchIcon class="size-4 animate-spin" />{$t('onboarding.clients.login.loading')}</p>
  {:else if selectedAuth?.methods.length}
    {#if loginSession?.step.type === 'device_code'}
      <div class="space-y-3">
        <p class="text-muted-foreground">{$t('onboarding.clients.login.waiting')}</p>
        <div class="flex items-center gap-2 rounded-md bg-surface px-3 py-2">
          <code class="flex-1 text-center font-mono text-2xl tracking-[0.16em] text-heading">{loginSession.step.user_code}</code>
          <Button variant="ghost" size="icon-sm" aria-label={$t('common.copy')} onclick={() => copyText(loginSession?.step.type === 'device_code' ? loginSession.step.user_code : '')}><CopyIcon /></Button>
        </div>
        <Button class="w-full" href={loginSession.step.verification_url} target="_blank" rel="noreferrer">{$t('onboarding.clients.login.openPage')}<ArrowSquareOutIcon data-icon="inline-end" /></Button>
        <Button variant="ghost" class="w-full" onclick={stopLogin}>{$t('onboarding.clients.login.cancel')}</Button>
      </div>
    {:else if loginSession?.step.type === 'browser_redirect'}
      <div class="space-y-3"><p class="text-muted-foreground">{loginSession.step.message ?? $t('onboarding.clients.login.waiting')}</p><Button class="w-full" href={loginSession.step.auth_url} target="_blank" rel="noreferrer">{$t('onboarding.clients.login.openPage')}<ArrowSquareOutIcon data-icon="inline-end" /></Button><Button variant="ghost" class="w-full" onclick={stopLogin}>{$t('onboarding.clients.login.cancel')}</Button></div>
    {:else if loginSession?.step.type === 'await_input'}
      <div class="space-y-3"><p class="text-muted-foreground">{loginSession.step.prompt}</p>
        {#each loginSession.step.fields as field (field.name)}
          <label class="grid gap-1 text-sm"><span>{field.label}</span>
            {#if field.kind.type === 'select'}
              <ResponsiveSelect value={sessionFields[field.name] ?? ''} onValueChange={value => sessionFields[field.name] = value} label={field.label} options={fieldOptions(field)} keyOf={option => option.value} labelOf={option => option.label}>
                {#snippet trigger()}<span>{fieldOptionLabel(field, sessionFields[field.name] ?? '')}</span>{/snippet}
                {#snippet option(option)}{option.label}{/snippet}
              </ResponsiveSelect>
            {:else}<Input type={field.kind.type === 'secret' ? 'password' : field.kind.type === 'url' ? 'url' : 'text'} value={sessionFields[field.name] ?? ''} oninput={event => sessionFields[field.name] = event.currentTarget.value} placeholder={field.placeholder} required={field.required} />{/if}
          </label>
        {/each}
        <div class="flex gap-2"><Button class="flex-1" onclick={sendLoginInput}>{$t('common.continue')}</Button><Button variant="outline" onclick={stopLogin}>{$t('onboarding.clients.login.cancel')}</Button></div>
      </div>
    {:else if loginSession?.state === 'failed' || loginSession?.state === 'timed_out'}
      <p class="text-warning" role="status">{loginSession.step.type === 'failed' ? loginSession.step.message : $t('onboarding.clients.login.expired')}</p>
    {:else}
      <p class="text-muted-foreground">{selectedAuth.state === 'expired' ? $t('onboarding.clients.login.expiredStatus', { name: selected?.name ?? '' }) : $t('onboarding.clients.login.signedOutStatus', { name: selected?.name ?? '' })}</p>
      {#if selectedAuth.methods.length > 1}
        <RadioGroup.Root bind:value={selectedMethodId} class="mt-3 gap-2" onchange={() => { loginFields = {} }}>
          {#each selectedAuth.methods as method (method.id)}<label class="flex cursor-pointer items-start gap-2 rounded-md border p-2"><RadioGroup.Item value={method.id} class="mt-0.5" /><span><span class="block text-sm font-medium">{method.label}</span>{#if method.description}<span class="block text-xs text-muted-foreground">{method.description}</span>{/if}</span></label>{/each}
        </RadioGroup.Root>
      {/if}
      {#if managedMethod?.start_fields.length}<div class="mt-3 space-y-3">{#each managedMethod.start_fields as field (field.name)}<label class="grid gap-1 text-sm"><span>{field.label}</span>{#if field.kind.type === 'select'}<ResponsiveSelect value={loginFields[field.name] ?? field.default_value ?? ''} onValueChange={value => loginFields[field.name] = value} label={field.label} options={fieldOptions(field)} keyOf={option => option.value} labelOf={option => option.label}>{#snippet trigger()}<span>{fieldOptionLabel(field, loginFields[field.name] ?? field.default_value ?? '')}</span>{/snippet}{#snippet option(option)}{option.label}{/snippet}</ResponsiveSelect>{:else}<Input type={field.kind.type === 'secret' ? 'password' : field.kind.type === 'url' ? 'url' : 'text'} value={loginFields[field.name] ?? field.default_value ?? ''} oninput={event => loginFields[field.name] = event.currentTarget.value} placeholder={field.placeholder} required={field.required} />{/if}{#if field.description}<span class="text-xs text-muted-foreground">{field.description}</span>{/if}</label>{/each}</div>{/if}
    {/if}
  {:else}
    <div class="space-y-3">
      <p class="text-muted-foreground" role="status">{selectedAuth?.state === 'unknown' ? $t('onboarding.clients.login.unknown', { name: selected?.name ?? '' }) : $t('onboarding.clients.login.signedOutStatus', { name: selected?.name ?? '' })}</p>
      {#if terminalCommand}
        <p class="text-sm">{$t('onboarding.clients.login.runOn', { device: deviceName })}</p>
        <div class="flex items-center gap-2 rounded-md bg-neutral-950 px-3 py-2 text-neutral-50">
          <code class="min-w-0 flex-1 overflow-x-auto font-mono text-xs">{terminalCommand}</code>
          <Button variant="ghost" size="icon-sm" class="text-neutral-50 hover:bg-white/10 hover:text-white" aria-label={$t('common.copy')} onclick={() => copyText(terminalCommand ?? '')}><CopyIcon /></Button>
        </div>
        <p class="text-xs text-muted-foreground">{$t('onboarding.clients.login.thenReturn')}</p>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet enableContent()}
  <p class="text-muted-foreground">{selected?.setup.status === 'required' ? $t('onboarding.clients.enable.setupLead') : $t('onboarding.clients.enable.lead')}</p>
{/snippet}

<Dialog.Root bind:open onOpenChange={next => { if (!next && open) dismiss() }}>
  <ResponsiveDialog mode="sheet" dismiss={dismiss} showCloseButton={false} interactOutsideBehavior="ignore" class="max-h-[calc(var(--visual-viewport-height)-54px)] gap-0 overflow-hidden p-0 sm:max-h-[740px] sm:max-w-[600px]" aria-describedby={undefined} onOpenAutoFocus={event => { if (document.activeElement?.matches('input, textarea, [contenteditable=true]')) event.preventDefault() }}>
    {#if view === 'pick'}
      <Dialog.Header class="border-b px-5 py-4 text-left">
        <Dialog.Title class="flex items-center gap-2 text-lg"><span class="flex size-8 items-center justify-center rounded-lg bg-primary/10 text-primary"><SparkleIcon class="size-5" weight="fill" /></span>{$t('onboarding.clients.title')}</Dialog.Title>
      </Dialog.Header>
      <div class="min-h-0 space-y-5 overflow-y-auto px-5 py-4">
        <p class="text-sm text-muted-foreground">{$t('onboarding.clients.lead', { device: deviceName })}</p>
        <div class={cn('flex gap-3 rounded-lg px-3 py-2.5', reason === 'unknown' ? 'bg-muted' : 'bg-warning/10')} role="status">
          {#if reason === 'unknown'}<QuestionIcon class="mt-0.5 size-5 shrink-0 text-muted-foreground" />{:else}<WarningCircleIcon class="mt-0.5 size-5 shrink-0 text-warning" />{/if}
          <div><p class="font-medium text-heading">{reason === 'unknown' ? $t('onboarding.clients.reason.unknownTitle', { names: unknownNames }) : $t('onboarding.clients.reason.title')}</p><p class="mt-0.5 text-xs text-muted-foreground">{reasonText()}</p></div>
        </div>

        {#if installed.length}
          <section class="space-y-1"><h2 class="pb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">{$t('onboarding.clients.group.onDevice')}</h2>
            {#each installed as client, index (client.id)}
              <div class="flex items-start gap-3 border-b py-3 last:border-b-0">
                <ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="mt-0.5 size-8" />
                <div class="min-w-0 flex-1 space-y-2">
                  <div class="flex min-w-0 items-center gap-2">
                    <div class="flex min-w-0 flex-1 items-center gap-2"><h3 class="truncate font-medium text-heading">{client.name}</h3><span class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">{authState(client)?.methods.length ? $t('onboarding.clients.login.web') : $t('onboarding.clients.login.terminal')}</span></div>
                    <Button class="shrink-0" size="sm" variant={index === 0 ? 'default' : 'outline'} onclick={() => choose(client)}>{rowAction(client)}</Button>
                  </div>
                  <ClientStageTrack stages={stages(client)} label={$t('onboarding.clients.progress')} />
                </div>
              </div>
            {/each}
          </section>
        {/if}

        {#if installable.length}
          <section class="space-y-1"><h2 class="pb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">{$t(installed.length ? 'onboarding.clients.group.installOthers' : 'onboarding.clients.group.install')}</h2>
            {#if installed.length && !installExpanded}
              <button class="flex w-full items-center gap-3 rounded-lg border px-3 py-3 text-left hover:bg-muted/50" onclick={() => installExpanded = true}><span class="flex -space-x-2">{#each installable.slice(0, 3) as client (client.id)}<ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-7 rounded-full bg-popover ring-2 ring-popover" />{/each}</span><span class="min-w-0 flex-1 truncate text-sm">{installable.map(client => client.name).join(' · ')}</span><CaretDownIcon class="size-4 text-muted-foreground" /></button>
            {:else}
              {#each installable as client (client.id)}
                <div class="flex items-center gap-3 border-b py-3 last:border-b-0"><ClientIcon clientType={client.client_type} acpAgentId={client.client_key} class="size-8 opacity-80" /><div class="min-w-0 flex-1"><h3 class="font-medium text-heading">{client.name}</h3><p class="truncate text-xs text-muted-foreground">{client.install?.methods.find(method => method.recommended)?.label ?? client.install?.methods[0]?.label}</p></div><Button size="sm" variant={installed.length ? 'outline' : 'default'} onclick={() => choose(client)}><DownloadSimpleIcon data-icon="inline-start" />{$t('onboarding.clients.action.install')}</Button></div>
              {/each}
            {/if}
          </section>
        {/if}
        <div class="flex flex-wrap gap-x-4 gap-y-2 text-sm"><button class="text-primary underline underline-offset-4" onclick={onManage}>{$t('onboarding.clients.manage')}</button>{#if installed[0]?.install?.auth.docs_url}<a class="inline-flex items-center gap-1 text-primary underline underline-offset-4" href={installed[0].install?.auth.docs_url ?? ''} target="_blank" rel="noreferrer">{$t('onboarding.clients.docs')}<ArrowSquareOutIcon class="size-3" /></a>{/if}</div>
      </div>
      <Dialog.Footer class="border-t px-5 py-4 sm:justify-end"><Button variant={reason === 'unknown' ? 'default' : 'ghost'} onclick={dismiss}>{$t('onboarding.clients.skip')}</Button></Dialog.Footer>
    {:else if view === 'flow' && selected}
      <Dialog.Header class="border-b px-5 py-4 text-left"><div class="flex items-center gap-3"><Button variant="ghost" size="icon-sm" aria-label={$t('common.back')} onclick={() => { view = 'pick'; error = '' }}><CaretLeftIcon /></Button><ClientIcon clientType={selected.client_type} acpAgentId={selected.client_key} class="size-7" /><div><Dialog.Title>{$t('onboarding.clients.flow.title', { name: selected.name })}</Dialog.Title><Dialog.Description>{selectedAuth?.methods.length ? $t('onboarding.clients.login.web') : $t('onboarding.clients.login.terminal')}</Dialog.Description></div></div></Dialog.Header>
      <div class="min-h-0 overflow-y-auto px-5 py-4"><p class="mb-5 text-sm text-muted-foreground">{$t('onboarding.clients.flow.lead', { name: selected.name, device: deviceName })}</p><OnboardingSteps {steps} label={$t('onboarding.clients.progress')} />{#if error}<p class="mt-4 text-sm text-destructive" role="alert">{error}</p>{/if}</div>
      <Dialog.Footer class="flex-col-reverse gap-2 border-t px-5 py-4 sm:flex-row sm:items-center sm:justify-between"><Button variant="ghost" onclick={dismiss}>{$t('onboarding.clients.skip')}</Button><div class="flex gap-2">
        {#if currentStep === 0}<Button disabled={!preferredMethod || installing} onclick={runInstall}>{#if installing}<CircleNotchIcon data-icon="inline-start" class="animate-spin" />{:else}<DownloadSimpleIcon data-icon="inline-start" />{/if}{$t('onboarding.clients.action.install')}</Button>
        {:else if currentStep === 1 && selectedAuth?.methods.length}<Button disabled={authLoading || (loginSession !== null && !['failed', 'cancelled', 'timed_out'].includes(loginSession.state))} onclick={runLogin}><KeyIcon data-icon="inline-start" />{loginSession?.state === 'failed' || loginSession?.state === 'timed_out' ? $t('onboarding.clients.login.retry') : $t('onboarding.clients.action.login')}</Button>
        {:else if currentStep === 1}<Button disabled={authLoading} onclick={confirmTerminal}><KeyIcon data-icon="inline-start" />{$t('onboarding.clients.login.signedInTerminal')}</Button>
        {:else if currentStep === 2}<Button disabled={enabling} onclick={runEnable}>{#if enabling}<CircleNotchIcon data-icon="inline-start" class="animate-spin" />{:else}<PowerIcon data-icon="inline-start" />{/if}{selected.setup.status === 'required' ? $t('onboarding.clients.action.enableSetup') : $t('onboarding.clients.action.enable')}</Button>{/if}
      </div></Dialog.Footer>
    {:else if view === 'done' && selected}
      <div class="flex min-h-72 flex-col items-center justify-center px-6 py-10 text-center"><span class="relative"><ClientIcon clientType={selected.client_type} acpAgentId={selected.client_key} class="size-16" /><CheckCircleIcon class="absolute -right-2 -bottom-2 size-7 rounded-full bg-popover text-success" weight="fill" /></span><Dialog.Title class="mt-5 text-xl">{$t('onboarding.clients.done.title', { name: selected.name })}</Dialog.Title><Dialog.Description class="mt-2 max-w-sm">{$t('onboarding.clients.done.lead', { name: selected.name, device: deviceName })}</Dialog.Description><ul class="mt-5 space-y-2 text-left text-sm text-muted-foreground"><li>✓ {$t('onboarding.clients.install.done', { version: selected.version ?? '' })}</li><li>✓ {terminalConfirmed.includes(selected.id) ? $t('onboarding.clients.login.signedInTerminalSum') : $t('onboarding.clients.login.signedIn')}</li><li>✓ {$t('onboarding.clients.enable.done')}</li></ul></div>
      <Dialog.Footer class="border-t px-5 py-4"><Button variant="outline" onclick={() => { view = 'pick'; selectedId = null }}>{$t('onboarding.clients.done.addAnother')}</Button><Button onclick={() => { open = false; onComplete() }}>{$t(mobile.current ? 'onboarding.clients.done.setupBot' : 'onboarding.clients.done.startSession')}</Button></Dialog.Footer>
    {/if}
  </ResponsiveDialog>
</Dialog.Root>
