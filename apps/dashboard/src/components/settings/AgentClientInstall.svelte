<script lang="ts">
  import { tick } from 'svelte'
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Item from '$lib/components/ui/item/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import * as RadioGroup from '$lib/components/ui/radio-group/index.js'
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import SheetHandle from '$lib/components/ui/overlay/SheetHandle.svelte'
  import { copyText } from '$lib/copyText'
  import { cn } from '$lib/utils'
  import type { AgentClientInstallMethod, AgentClientView } from '../../api/types'
  import type { createAgentClients } from '../../features/settings/agentClients.svelte'
  import {
    lastOutputLine,
    missingRequired,
    optionalWarnings,
    preferredInstallMethod,
  } from '../../features/settings/agentClientInstall'

  let {
    settings,
    client,
    mobile = false,
  }: { settings: ReturnType<typeof createAgentClients>; client: AgentClientView; mobile?: boolean } = $props()

  let confirmOpen = $state(false)
  let outputOpen = $state(false)
  let cancelOpen = $state(false)
  let selectedMethodId = $state('')
  let submitting = $state(false)
  let submitError = $state('')
  let outputNode = $state<HTMLElement | null>(null)
  let following = $state(true)

  const operation = $derived(settings.installOperationFor(client))
  const methods = $derived(client.install?.methods ?? [])
  const preferred = $derived(preferredInstallMethod(client))
  const selectedMethod = $derived(methods.find((method) => method.id === selectedMethodId) ?? preferred)
  const allBlocked = $derived(methods.length > 0 && methods.every((method) => !method.ready))
  const optionalMissing = $derived(preferred ? optionalWarnings(preferred) : [])
  const show = $derived(
    (client.detection.status === 'not_detected' && methods.length > 0) || operation !== null,
  )
  const activeOther = $derived(
    settings.activeInstall?.status === 'running' && !operation
      ? settings.clients.find(
          (entry) =>
            entry.client_type === settings.activeInstall?.client_type &&
            entry.client_key === settings.activeInstall?.client_key,
        )
      : null,
  )
  const latestLine = $derived(operation ? lastOutputLine(operation.output) : '')
  const steps = $derived([
    { phase: 'preflight', label: $t('clients.install.step.preflight') },
    { phase: 'installing', label: $t('clients.install.step.installing') },
    { phase: 'verifying', label: $t('clients.install.step.verifying') },
    ...(client.setup_command
      ? [{ phase: 'setting_up', label: $t('clients.install.step.setting_up') }]
      : []),
  ])
  const phaseIndex = $derived(
    operation?.phase === 'done'
      ? steps.length
      : steps.findIndex((step) => step.phase === operation?.phase),
  )

  const row = 'min-h-14 flex-nowrap gap-3 rounded-none px-0 py-2.5'
  const title = $derived(cn('text-heading', mobile && 'text-[15px]'))
  const description = $derived(cn('line-clamp-none break-words text-xs [overflow-wrap:anywhere]', mobile && 'text-[13px] leading-[18px]'))
  const button = $derived(cn(mobile && 'h-9 px-3 text-sm'))

  function openConfirmation(method: AgentClientInstallMethod | null = preferred) {
    selectedMethodId = method?.ready ? method.id : (methods.find((entry) => entry.ready)?.id ?? '')
    submitError = ''
    confirmOpen = true
  }

  async function submit() {
    if (!selectedMethod?.ready) return
    submitting = true
    submitError = ''
    try {
      await settings.install(client, selectedMethod)
      confirmOpen = false
    } catch (cause) {
      submitError = cause instanceof Error ? cause.message : String(cause)
    } finally {
      submitting = false
    }
  }

  async function copyOutput() {
    if (operation?.output && (await copyText(operation.output))) toast.success($t('common.copied'))
  }

  function updateFollowing() {
    if (!outputNode) return
    following = outputNode.scrollHeight - outputNode.scrollTop - outputNode.clientHeight < 32
  }

  async function jumpToLatest() {
    following = true
    await tick()
    if (outputNode) outputNode.scrollTop = outputNode.scrollHeight
  }

  $effect(() => {
    if (outputOpen && following && operation?.output !== undefined)
      void tick().then(() => {
        if (outputNode) outputNode.scrollTop = outputNode.scrollHeight
      })
  })

  $effect(() => {
    if (cancelOpen && operation && operation.status !== 'running') cancelOpen = false
  })
</script>

{#if show}
  <Item.Root class={cn(row, 'items-start')}>
    <Item.Content class="min-w-0 gap-1.5">
      <Item.Title class={title}>{$t('clients.install.title')}</Item.Title>
      <Item.Description class={description}>
        <span role="status">
          {#if activeOther}
            <span class="text-warning">{$t('clients.install.busyOther', { name: activeOther.name })}</span>
          {:else if operation?.status === 'running'}
            <span class="inline-flex items-center gap-1.5">
              <CircleNotchIcon class="size-3 shrink-0 animate-spin" />
              {operation.cancel_requested
                ? $t('clients.install.cancelling')
                : $t(`clients.install.phase.${operation.phase}`)}
            </span>
          {:else if operation?.status === 'failed'}
            <span class="text-destructive">{$t('clients.install.failed', { error: operation.error ?? $t('clients.install.unknownError') })}</span>
          {:else if operation?.status === 'cancelled'}
            {$t('clients.install.cancelled')}
          {:else if operation?.status === 'succeeded'}
            <span class="inline-flex items-center gap-1.5 text-success"><CheckIcon class="size-3.5" />{$t(client.setup_command ? 'clients.install.succeededSetup' : 'clients.install.succeeded')}</span>
          {:else if allBlocked}
            <span class="text-warning">{$t('clients.install.blocked')}</span>
          {:else if preferred}
            {$t('clients.install.summary', { method: preferred.label, target: preferred.target_summary })}
            {#if methods.length > 1}{$t('clients.install.moreMethods', { count: methods.length - 1 })}{/if}
          {/if}
        </span>
      </Item.Description>

      {#if operation}
        <ol class="flex flex-wrap items-center gap-1.5 pt-1 text-xs text-muted-foreground" aria-label={$t('clients.install.progress')}>
          {#each steps as step, index (step.phase)}
            {#if index > 0}<span aria-hidden="true">—</span>{/if}
            <li
              class={cn(
                'inline-flex items-center gap-1',
                operation.status === 'failed' && index === phaseIndex && 'text-destructive',
                (operation.status === 'succeeded' || index < phaseIndex) && 'text-success',
              )}
              aria-current={operation.status === 'running' && index === phaseIndex ? 'step' : undefined}
            >
              {#if operation.status === 'running' && index === phaseIndex}
                <CircleNotchIcon class="size-3 animate-spin" />
              {:else if operation.status === 'failed' && index === phaseIndex}
                <XIcon class="size-3" /><span class="sr-only">{$t('clients.install.stepFailed')}</span>
              {:else if operation.status === 'succeeded' || index < phaseIndex}
                <CheckIcon class="size-3" />
              {:else}
                <span class="size-2 rounded-full border border-current" aria-hidden="true"></span>
              {/if}
              {step.label}
            </li>
          {/each}
        </ol>
        {#if latestLine}
          <p class={cn('truncate rounded-md bg-muted px-2 py-1 font-mono text-xs', operation.status === 'failed' && 'text-destructive')} aria-live="off" title={latestLine}>{latestLine}</p>
        {/if}
        {#if settings.installConnectionError}
          <p class="text-warning">{$t('clients.install.reconnecting')}</p>
        {:else if operation.status === 'running'}
          <p>{$t('clients.install.leaveHint')}</p>
        {:else if operation.status === 'failed'}
          <p>{$t('clients.install.failureHint')}</p>
        {/if}
      {:else if allBlocked && preferred}
        <ul class="space-y-1 pt-1">
          {#each missingRequired(preferred) as prerequisite (prerequisite.id)}
            <li class="flex gap-1.5 text-destructive"><XIcon class="mt-0.5 size-3 shrink-0" /><span>{prerequisite.label}{prerequisite.missing_hint ? ` · ${prerequisite.missing_hint}` : ''}</span></li>
          {/each}
        </ul>
        <p>{$t('clients.install.pathHint')}</p>
      {:else if optionalMissing.length}
        <ul class="space-y-1 pt-1">
          {#each optionalMissing as prerequisite (prerequisite.id)}
            <li class="flex gap-1.5 text-warning"><WarningCircleIcon class="mt-0.5 size-3 shrink-0" /><span>{prerequisite.label}{prerequisite.missing_hint ? ` · ${prerequisite.missing_hint}` : ''}</span></li>
          {/each}
        </ul>
      {/if}

      {#if mobile}
        <div class="grid grid-cols-2 gap-2 pt-2">
          {#if operation}
            <Button variant="outline" class="h-11" onclick={() => (outputOpen = true)}>{$t('clients.viewOutput')}</Button>
            {#if operation.status === 'running'}
              <Button variant="outline" class="h-11 text-destructive" disabled={operation.cancel_requested} onclick={() => (cancelOpen = true)}>{$t('clients.install.cancel')}</Button>
            {:else if operation.status !== 'succeeded'}
              <Button class="h-11" disabled={settings.operationBusy} onclick={() => openConfirmation(methods.find((method) => method.id === operation.method_id) ?? preferred)}>{$t(operation.status === 'cancelled' ? 'clients.install.reinstall' : 'common.retry')}</Button>
            {/if}
          {:else}
            <Button class="col-span-2 h-11" disabled={allBlocked || settings.operationBusy || settings.installLoading} onclick={() => openConfirmation()}><DownloadSimpleIcon data-icon="inline-start" />{$t('clients.install.actionNamed', { name: client.name })}</Button>
          {/if}
        </div>
      {/if}
    </Item.Content>

    {#if !mobile}
      <Item.Actions class="gap-1">
        {#if operation}
          <Button variant="ghost" size="sm" class={button} onclick={() => (outputOpen = true)}>{$t('clients.viewOutput')}</Button>
          {#if operation.status === 'running'}
            <Button variant="ghost" size="sm" class={cn(button, 'text-destructive')} disabled={operation.cancel_requested} onclick={() => (cancelOpen = true)}>{$t('clients.install.cancel')}</Button>
          {:else if operation.status !== 'succeeded'}
            <Button size="sm" class={button} disabled={settings.operationBusy} onclick={() => openConfirmation(methods.find((method) => method.id === operation.method_id) ?? preferred)}>{$t(operation.status === 'cancelled' ? 'clients.install.reinstall' : 'common.retry')}</Button>
          {/if}
        {:else if allBlocked}
          <Button variant="ghost" size="sm" class={button} disabled={settings.refreshing} onclick={settings.refresh}><ArrowClockwiseIcon data-icon="inline-start" class={settings.refreshing ? 'animate-spin' : undefined} />{$t('clients.install.recheck')}</Button>
        {:else}
          <Button size="sm" class={button} disabled={settings.operationBusy || settings.installLoading} onclick={() => openConfirmation()}><DownloadSimpleIcon data-icon="inline-start" />{$t('clients.install.action')}</Button>
        {/if}
      </Item.Actions>
    {/if}
  </Item.Root>
{/if}

<Dialog.Root bind:open={confirmOpen}>
  <ResponsiveDialog mode="sheet" dismiss={() => (confirmOpen = false)} class="sm:max-w-lg" aria-describedby={undefined} onOpenAutoFocus={(event) => event.preventDefault()}>
    <Dialog.Header>
      <Dialog.Title>{$t('clients.install.confirmTitle', { name: client.name })}</Dialog.Title>
      <Dialog.Description>{$t('clients.install.confirmLead')}</Dialog.Description>
    </Dialog.Header>
    {#if methods.length > 1}
      <RadioGroup.Root bind:value={selectedMethodId} class="gap-2">
        {#each methods as method (method.id)}
          <label class={cn('flex cursor-pointer items-start gap-3 rounded-lg border p-3', selectedMethodId === method.id && 'border-primary bg-primary/5', !method.ready && 'cursor-not-allowed opacity-55')}>
            <RadioGroup.Item value={method.id} disabled={!method.ready} class="mt-0.5" />
            <span class="min-w-0 flex-1">
              <span class="flex items-center gap-2 text-sm font-medium text-heading">{method.label}{#if method.recommended}<Badge variant="secondary">{$t('clients.install.recommended')}</Badge>{/if}</span>
              <span class={cn('mt-0.5 block text-xs text-muted-foreground', !method.ready && 'text-destructive')}>{method.ready ? method.source_summary : $t('clients.install.missing', { list: missingRequired(method).map((item) => item.label).join('、') })}</span>
            </span>
          </label>
        {/each}
      </RadioGroup.Root>
    {/if}
    {#if selectedMethod}
      <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm">
        {#if methods.length === 1}<dt class="text-muted-foreground">{$t('clients.install.method')}</dt><dd>{selectedMethod.label}</dd>{/if}
        <dt class="text-muted-foreground">{$t('clients.install.source')}</dt><dd class="break-all font-mono text-xs">{selectedMethod.source_summary}</dd>
        <dt class="text-muted-foreground">{$t('clients.install.target')}</dt><dd class="break-all font-mono text-xs">{selectedMethod.target_summary}</dd>
        <dt class="text-muted-foreground">{$t('clients.install.after')}</dt><dd>{$t(client.setup_command ? 'clients.install.afterSetup' : 'clients.install.afterBasic')}</dd>
      </dl>
      <div>
        <h4 class="mb-2 text-sm font-medium text-heading">{$t('clients.install.checks')}</h4>
        <ul class="space-y-1.5 text-sm">
          {#each selectedMethod.prerequisites as prerequisite (prerequisite.id)}
            <li class={cn('flex items-start gap-2', prerequisite.satisfied ? 'text-success' : prerequisite.required ? 'text-destructive' : 'text-warning')}>
              {#if prerequisite.satisfied}<CheckIcon class="mt-0.5 size-3.5 shrink-0" />{:else if prerequisite.required}<XIcon class="mt-0.5 size-3.5 shrink-0" />{:else}<WarningCircleIcon class="mt-0.5 size-3.5 shrink-0" />{/if}
              <span>{prerequisite.label}{#if !prerequisite.required} {$t('clients.install.optional')}{/if}{#if !prerequisite.satisfied && prerequisite.missing_hint}<span class="block text-xs text-muted-foreground">{prerequisite.missing_hint}</span>{/if}</span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if submitError}<p role="alert" class="text-sm text-destructive">{submitError}</p>{/if}
    <Dialog.Footer class="max-sm:flex-col-reverse">
      <Button variant="outline" disabled={submitting} onclick={() => (confirmOpen = false)}>{$t('common.cancel')}</Button>
      <Button autofocus disabled={!selectedMethod?.ready || submitting} onclick={submit}>{#if submitting}<CircleNotchIcon data-icon="inline-start" class="animate-spin" />{/if}{$t('clients.install.action')}</Button>
    </Dialog.Footer>
  </ResponsiveDialog>
</Dialog.Root>

<Dialog.Root bind:open={outputOpen}>
  <ResponsiveDialog mode="sheet" dismiss={() => (outputOpen = false)} class="sm:max-w-2xl" aria-describedby={undefined}>
    <Dialog.Header>
      <div class="flex items-start justify-between gap-4 pr-7">
        <div><Dialog.Title>{$t('clients.install.output')} · {client.name}</Dialog.Title><Dialog.Description>{#if operation?.status === 'running'}<span class="inline-flex items-center gap-1.5"><span class="size-2 animate-pulse rounded-full bg-success"></span>{$t('clients.install.live')} · {$t(`clients.install.phase.${operation.phase}`)}</span>{:else if operation}{$t(`clients.install.outputStatus.${operation.status}`)}{/if}</Dialog.Description></div>
        <Button variant="ghost" size="icon-sm" aria-label={$t('clients.install.copyOutput')} title={$t('clients.install.copyOutput')} onclick={copyOutput}><CopyIcon /></Button>
      </div>
    </Dialog.Header>
    <div class="relative min-h-0">
      <pre bind:this={outputNode} onscroll={updateFollowing} class="max-h-[60vh] min-h-48 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-3 font-mono text-xs leading-5">{operation?.output}</pre>
      {#if !following}<Button variant="outline" size="sm" class="absolute right-3 bottom-3 shadow-md" onclick={jumpToLatest}>{$t('clients.install.jumpLatest')}</Button>{/if}
    </div>
  </ResponsiveDialog>
</Dialog.Root>

<AlertDialog.Root bind:open={cancelOpen}>
  <AlertDialog.Content class={mobile ? 'inset-x-0 top-auto bottom-[var(--visual-viewport-bottom,0px)] max-w-none translate-x-0 translate-y-0 rounded-b-none data-[size=default]:max-w-none data-[size=default]:sm:max-w-none' : 'ring-0 shadow-lg'}>
    {#if mobile}<SheetHandle dismiss={() => (cancelOpen = false)} />{/if}
    <AlertDialog.Header><AlertDialog.Title>{$t('clients.install.cancelConfirm', { name: client.name })}</AlertDialog.Title><AlertDialog.Description>{$t('clients.install.cancelBody')}</AlertDialog.Description></AlertDialog.Header>
    <AlertDialog.Footer><AlertDialog.Cancel>{$t('clients.install.keepGoing')}</AlertDialog.Cancel><AlertDialog.Action variant="destructive" disabled={settings.cancellingInstall || operation?.cancel_requested} onclick={(event) => { event.preventDefault(); void settings.cancelInstall() }}>{settings.cancellingInstall || operation?.cancel_requested ? $t('clients.install.cancelling') : $t('clients.install.cancelAction')}</AlertDialog.Action></AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
