<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowCounterClockwiseIcon from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import WrenchIcon from 'phosphor-svelte/lib/WrenchIcon'
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import type { createAgentClients } from '../../features/settings/agentClients.svelte'
  import SheetHandle from '$lib/components/ui/overlay/SheetHandle.svelte'
  let { settings, viewOutput }: { settings: ReturnType<typeof createAgentClients>; viewOutput?: () => void } =
    $props()
  let resetOpen = $state(false)
</script>

{#snippet restoreButton()}
  <Button type="button" variant="ghost" disabled={settings.busy} onclick={() => (resetOpen = true)}
    ><ArrowCounterClockwiseIcon data-icon="inline-start" />{$t('clients.restoreDefault')}</Button
  >
{/snippet}
{#if settings.selected}
  <div class="min-w-0 space-y-6">
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0 space-y-1">
        <h3 class:sr-only={!!viewOutput} class="truncate text-lg font-semibold text-heading">
          {settings.selected.name}
        </h3>
        {#if settings.selected.version}<p class="text-sm text-muted-foreground">
            {$t('clients.version', { version: settings.selected.version })}
          </p>{/if}
        <div role="status" class="space-y-1 pt-2 text-sm">
          <p class="flex items-center gap-1.5">
            {#if settings.selected.detection.status === 'detected'}<CheckCircleIcon class="size-4 text-success" />
            {:else if settings.selected.detection.status === 'not_detected'}<WarningCircleIcon
                class="size-4 text-muted-foreground"
              />{/if}
            {$t(`clients.detection.${settings.selected.detection.status}`)}
          </p>
          {#if settings.selected.detection.detail}<p class="break-words text-xs text-muted-foreground">
              {settings.selected.detection.detail}
            </p>{/if}
        </div>
      </div>
      <div class="flex shrink-0 items-center gap-2 pt-1">
        <Label for="client-enabled" class="text-sm text-muted-foreground">{$t('common.enabled')}</Label>
        <Switch
          id="client-enabled"
          checked={settings.selected.enabled}
          disabled={settings.busy}
          onCheckedChange={() => settings.selected && settings.toggle(settings.selected)}
        />
      </div>
    </div>

    {#if settings.selected.setup_command}
      <section class="space-y-4 rounded-lg border bg-card p-4" aria-label={$t('clients.setup')}>
        <div class="flex flex-wrap items-start justify-between gap-4">
          <div class="space-y-1.5">
            <h4 class="flex items-center gap-2 text-sm font-medium">
              <WrenchIcon class="size-4 text-muted-foreground" />
              {$t('clients.setup')}
            </h4>
            <p class="text-sm text-muted-foreground">{$t('clients.setupDescription')}</p>
            <code class="inline-block rounded bg-muted px-2 py-1 text-xs">{settings.selected.setup_command}</code>
          </div>
          <Button
            disabled={settings.busy || settings.setupLoading || !!settings.setupConnectionError}
            onclick={() => settings.selected && settings.setup(settings.selected)}
          >
            {#if settings.startingSetup || settings.selectedSetup?.status === 'running'}<CircleNotchIcon
                data-icon="inline-start"
                class="animate-spin"
              />{$t('clients.settingUp')}
            {:else}<WrenchIcon data-icon="inline-start" />{$t('clients.runSetup')}{/if}
          </Button>
        </div>
        {#if settings.selectedSetup}
          <div class="space-y-3 border-t pt-3">
            <div role="status" class="flex items-center gap-2 text-sm">
              {#if settings.selectedSetup.status === 'running'}<CircleNotchIcon
                  class="size-4 animate-spin text-muted-foreground"
                />{$t('clients.setupRunning')}
              {:else if settings.selectedSetup.status === 'succeeded'}<CheckCircleIcon
                  class="size-4 text-success"
                />{$t('clients.setupSucceeded')}
              {:else}<WarningCircleIcon class="size-4 text-destructive" />{$t('clients.setupFailed')}{/if}
            </div>
            {#if settings.selectedSetup.error}<p class="text-sm text-destructive">
                {settings.selectedSetup.error}
              </p>{/if}
            {#if settings.selectedSetup.output}
              {#if viewOutput}<Button variant="outline" onclick={viewOutput}>{$t('clients.setupOutput')}</Button>
              {:else}<pre
                  aria-label={$t('clients.setupOutput')}
                  class="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-3 font-mono text-xs leading-5">{settings
                    .selectedSetup.output}</pre>{/if}
            {/if}
          </div>
        {/if}
        {#if settings.setupConnectionError}
          <p role="status" class="text-sm text-muted-foreground">
            {$t('clients.setupReconnecting')}
            {settings.setupConnectionError}
          </p>
        {/if}
      </section>
    {/if}

    {#if settings.selected.has_record}{@render restoreButton()}{/if}
  </div>
{/if}
<AlertDialog.Root bind:open={resetOpen}>
  <AlertDialog.Content class={viewOutput ? 'inset-x-0 top-auto bottom-[var(--visual-viewport-bottom,0px)] max-w-none translate-x-0 translate-y-0 rounded-b-none data-[size=default]:max-w-none data-[size=default]:sm:max-w-none' : undefined}>
    {#if viewOutput}<SheetHandle dismiss={() => (resetOpen = false)} />{/if}
    <AlertDialog.Header
      ><AlertDialog.Title>{$t('clients.restoreConfirm')}</AlertDialog.Title><AlertDialog.Description
        >{$t('clients.restoreBefore')}
        {settings.selected?.name}{$t('clients.restoreAfter')}</AlertDialog.Description
      ></AlertDialog.Header
    >
    <AlertDialog.Footer
      ><AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel><AlertDialog.Action
        onclick={() => {
          resetOpen = false
          if (settings.selected) void settings.reset(settings.selected)
        }}>{$t('clients.restoreDefault')}</AlertDialog.Action
      ></AlertDialog.Footer
    >
  </AlertDialog.Content>
</AlertDialog.Root>
