<script lang="ts">
  import { t, currentLocale } from '$lib/i18n'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import WrenchIcon from 'phosphor-svelte/lib/WrenchIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import * as Item from '$lib/components/ui/item/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import SheetHandle from '$lib/components/ui/overlay/SheetHandle.svelte'
  import { createCopyFeedback } from '$lib/copyFeedback.svelte'
  import { formatRelativeTime } from '$lib/format'
  import { dashboardPath, navigate } from '$lib/navigation'
  import { cn } from '$lib/utils'
  import type { AgentClientView } from '../../api/types'
  import type { createAgentClients } from '../../features/settings/agentClients.svelte'
  import AgentClientInstall from './AgentClientInstall.svelte'
  import AgentClientAccount from './AgentClientAccount.svelte'

  let {
    settings,
    client,
    mobile = false,
  }: { settings: ReturnType<typeof createAgentClients>; client: AgentClientView; mobile?: boolean } = $props()
  let resetOpen = $state(false)
  let outputOpen = $state(false)
  const copyFeedback = createCopyFeedback()

  const profilesPath = '/settings/agent-profiles'
  const detected = $derived(client.detection.status !== 'not_detected')
  const operation = $derived(settings.operationFor(client))
  const installOperation = $derived(settings.installOperationFor(client))
  const profileCount = $derived(settings.profileCount(client))
  const setupDisabled = $derived(settings.operationBusy || settings.setupLoading || !!settings.setupConnectionError)
  const showIntegration = $derived(!!client.setup_command && client.setup.status !== 'not_required')

  const row = 'min-h-14 flex-nowrap gap-3 rounded-none px-0 py-2.5'
  const title = $derived(cn('text-heading', mobile && 'text-15'))
  const description = $derived(cn('line-clamp-1 text-xs', mobile && 'text-13 leading-[18px]'))
  const wrap = 'line-clamp-none break-words [overflow-wrap:anywhere]'
  const separator = 'my-0 bg-foreground/6 dark:bg-foreground/7'
  const groupTitle = $derived(
    cn('mb-0.5 text-xs font-medium text-muted-foreground', mobile && 'text-13 leading-[18px]'),
  )
  const button = $derived(cn(mobile && 'h-9 px-3 text-sm'))
  // Borderless buttons at the row end pull their label to the same edge as switches and values.
  const lastGhost = $derived(mobile ? '-mr-3' : '-mr-2.5')

  async function copyLocation() {
    await copyFeedback.copy(client.detection.detail)
  }
</script>

<div class="min-w-0">
  <section>
    <Item.Group class="gap-0">
      <AgentClientInstall {settings} {client} {mobile} />
      {#if (client.detection.status === 'not_detected' && (client.install?.methods.length ?? 0) > 0) || installOperation}
        <Item.Separator class={separator} />
      {/if}
      <Item.Root class={row}>
        <Item.Content class="min-w-0 gap-0.5">
          <Item.Title class={title}>{$t('clients.enable')}</Item.Title>
          <Item.Description class={cn(description, wrap)}>{$t('clients.enableDescription')}{#if mobile} {$t('clients.sessionsUnchanged')}{/if}</Item.Description>
        </Item.Content>
        <Item.Actions>
          <Switch
            aria-label={$t('clients.enableNamed', { name: client.name })}
            class={cn(mobile && 'after:-inset-x-2.5 after:-inset-y-3.5')}
            checked={client.enabled}
            disabled={settings.saving(client) || installOperation?.status === 'running'}
            onCheckedChange={() => settings.toggle(client)}
          />
        </Item.Actions>
      </Item.Root>
      {#if detected}
        <Item.Separator class={separator} />
        <Item.Root
          class={cn(
            row,
            '-mx-3 w-auto rounded-item px-3',
            mobile ? '[a]:hover:bg-transparent [a]:active:bg-muted' : '[a]:hover:bg-muted/60',
          )}
        >
          {#snippet child({ props })}
            <a
              {...props}
              href={dashboardPath(profilesPath)}
              onclick={(event) => {
                event.preventDefault()
                void navigate(profilesPath)
              }}
            >
              <Item.Content class="min-w-0 gap-0.5">
                <Item.Title class={title}>{$t('clients.profiles')}</Item.Title>
                <Item.Description class={description}>{$t('clients.profilesDescription')}</Item.Description>
              </Item.Content>
              <Item.Actions class="gap-1.5 text-muted-foreground">
                <span class={description}
                  >{profileCount
                    ? $t('clients.profileCount', { count: profileCount })
                    : $t('clients.profilesUnused')}</span
                >
                <CaretRightIcon class="size-4" />
              </Item.Actions>
            </a>
          {/snippet}
        </Item.Root>
      {/if}
    </Item.Group>
  </section>

  <section class="mt-7">
    <h3 class={groupTitle}>{$t('clients.statusGroup')}</h3>
    <Item.Group class="gap-0">
      <Item.Root class={row}>
        <Item.Content class="min-w-0 gap-0.5">
          <Item.Title class={title}>{$t('clients.location')}</Item.Title>
          {#if client.detection.detail}
            <Item.Description
              class={cn(description, 'font-mono', mobile && wrap)}
              title={client.detection.detail}>{client.detection.detail}</Item.Description
            >
          {:else}
            <Item.Description class={cn(description, wrap)}
              >{client.detection.status === 'unknown'
                ? $t('clients.status.detectionUnknown')
                : $t('clients.locationMissing')}</Item.Description
            >
          {/if}
        </Item.Content>
        {#if client.detection.detail}
          <Item.Actions>
            <Button
              variant="ghost"
              size={mobile ? 'icon' : 'icon-sm'}
              class={cn(mobile && 'size-9 -mr-2.5')}
              aria-label={$t('clients.copyLocation')}
              title={$t('clients.copyLocation')}
              onclick={copyLocation}>{#if copyFeedback.confirmed}<CheckIcon />{:else}<CopyIcon />{/if}</Button
            >
          </Item.Actions>
        {/if}
      </Item.Root>
      {#if detected}
        <Item.Separator class={separator} />
        <Item.Root class={row}>
          <Item.Content class="min-w-0 gap-0.5">
            <Item.Title class={title}>{$t('clients.versionLabel')}</Item.Title>
            <Item.Description class={description}
              >{client.version_checked_at
                ? $t('clients.versionChecked', { time: formatRelativeTime(client.version_checked_at, $currentLocale) })
                : $t('clients.versionUnchecked')}</Item.Description
            >
          </Item.Content>
          <Item.Actions>
            <span class={cn('font-mono text-13 text-foreground', mobile && 'text-sm')}>{client.version ?? '—'}</span>
          </Item.Actions>
        </Item.Root>
      {/if}
      {#if showIntegration}
        <Item.Separator class={separator} />
        <Item.Root class={row}>
          <Item.Content class="min-w-0 gap-0.5">
            <Item.Title class={title}>{$t('clients.integration')}</Item.Title>
            <Item.Description class={cn(description, wrap)}>
              {#if operation?.status === 'running'}
                <span class="inline-flex items-center gap-1.5"
                  ><CircleNotchIcon class="size-3 shrink-0 animate-spin" />{$t('clients.setupRunning')}</span
                >
              {:else if operation?.status === 'failed'}
                <span class="text-destructive"
                  >{operation.error
                    ? $t('clients.setupFailedWith', { error: operation.error })
                    : $t('clients.status.setupFailed')}</span
                >
              {:else if operation?.status === 'succeeded'}
                {$t('clients.setupSucceeded')}
              {:else if client.setup.status === 'required'}
                <span class="block text-warning">{$t('clients.setupRequired', { detail: client.setup.detail ?? '' })}</span>
                <span class="block"
                  >{$t('clients.setupTerminal')} <code class="font-mono text-foreground">{client.setup_command}</code></span
                >
              {:else if client.setup.status === 'unknown'}
                <span class="text-warning">{client.setup.detail ?? $t('clients.status.setupUnknown')}</span>
              {:else}
                {$t('clients.setupInitialized')}
              {/if}
              {#if settings.setupConnectionError}
                <span role="status" class="block">{$t('clients.setupReconnecting')}</span>
              {/if}
            </Item.Description>
          </Item.Content>
          <Item.Actions class="gap-1">
            {#if operation?.id}
              <Button variant="ghost" size="sm" class={button} onclick={() => (outputOpen = true)}
                >{$t('clients.viewOutput')}</Button
              >
            {/if}
            {#if operation?.status === 'running'}
              <Button variant="ghost" size="sm" class={cn(button, lastGhost)} disabled
                ><CircleNotchIcon data-icon="inline-start" class="animate-spin" />{$t('clients.settingUp')}</Button
              >
            {:else if !operation && client.setup.status === 'required'}
              <Button size="sm" class={button} disabled={setupDisabled} onclick={() => settings.setup(client)}
                ><WrenchIcon data-icon="inline-start" />{$t('clients.runSetup')}</Button
              >
            {:else}
              <Button
                variant="ghost"
                size="sm"
                class={cn(button, lastGhost)}
                disabled={setupDisabled}
                onclick={() => settings.setup(client)}
                >{operation?.status === 'failed'
                  ? $t('common.retry')
                  : operation?.status === 'succeeded' || client.setup.status === 'initialized'
                    ? $t('clients.reinitialize')
                    : $t('clients.runSetup')}</Button
              >
            {/if}
          </Item.Actions>
        </Item.Root>
      {/if}
      {#if detected}
        <Item.Separator class={separator} />
        <AgentClientAccount {client} {mobile} />
      {/if}
    </Item.Group>
  </section>

  {#if client.has_record}
    <section class="mt-8">
      <Item.Group class="gap-0">
        <Item.Root class={row}>
          <Item.Content class="min-w-0 gap-0.5">
            <Item.Title class={title}>{$t('clients.restoreDefault')}</Item.Title>
            <Item.Description class={cn(description, mobile && wrap)}>{$t('clients.restoreDescription')}</Item.Description>
          </Item.Content>
          <Item.Actions>
            <Button
              variant="ghost"
              size="sm"
              class={cn(button, lastGhost, 'text-destructive hover:bg-destructive/10 hover:text-destructive')}
              disabled={settings.saving(client) || settings.operationBusy}
              onclick={() => (resetOpen = true)}>{$t('clients.restoreDefault')}</Button
            >
          </Item.Actions>
        </Item.Root>
      </Item.Group>
    </section>
  {/if}
</div>

{#if mobile}
  <Sheet.Root bind:open={outputOpen}
    ><BottomSheet dismiss={() => (outputOpen = false)} aria-describedby={undefined}
      ><Sheet.Title>{$t('clients.setupOutput')}</Sheet.Title>
      <pre class="overflow-auto whitespace-pre-wrap p-3 text-xs">{operation?.output}</pre></BottomSheet
    ></Sheet.Root
  >
{:else}
  <Dialog.Root bind:open={outputOpen}>
    <Dialog.Content class="ring-0 shadow-lg sm:max-w-2xl" aria-describedby={undefined}>
      <Dialog.Header><Dialog.Title>{$t('clients.setupOutput')} · {client.name}</Dialog.Title></Dialog.Header>
      <pre
        class="max-h-[60vh] overflow-auto whitespace-pre-wrap break-words rounded-item bg-muted p-3 font-mono text-xs leading-5">{operation?.output}</pre>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<AlertDialog.Root bind:open={resetOpen}>
  <AlertDialog.Content
    class={mobile
      ? 'inset-x-0 top-auto bottom-[var(--visual-viewport-bottom,0px)] max-w-none translate-x-0 translate-y-0 rounded-b-none data-[size=default]:max-w-none data-[size=default]:sm:max-w-none'
      : 'ring-0 shadow-lg'}
  >
    {#if mobile}<SheetHandle dismiss={() => (resetOpen = false)} />{/if}
    <AlertDialog.Header
      ><AlertDialog.Title>{$t('clients.restoreConfirm')}</AlertDialog.Title><AlertDialog.Description
        >{$t('clients.restoreBefore')}
        {client.name}{$t('clients.restoreAfter')}</AlertDialog.Description
      ></AlertDialog.Header
    >
    <AlertDialog.Footer
      ><AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel><AlertDialog.Action
        onclick={() => {
          resetOpen = false
          void settings.reset(client)
        }}>{$t('clients.restoreDefault')}</AlertDialog.Action
      ></AlertDialog.Footer
    >
  </AlertDialog.Content>
</AlertDialog.Root>
