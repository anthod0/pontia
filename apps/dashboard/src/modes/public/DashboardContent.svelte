<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import { page } from '$app/state'
  import { dashboardRelativePath } from '$lib/navigation'
  import { useDashboardHost } from '../../device/hostContext'
  import ComposerDeviceSelect from './ComposerDeviceSelect.svelte';
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
  import { Button } from '$lib/components/ui/button/index.js';
  import DeviceStatus from '../../components/devices/DeviceStatus.svelte';
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import NewChatPage from '../../pages/NewChatPage.svelte'
  import DashboardEntryPage from '../../pages/DashboardEntryPage.svelte';
  import BotEntryPage from '../../pages/BotEntryPage.svelte';
  import NewChatLayout from '../../components/chat/NewChatLayout.svelte';
  import NewChatComposerPlaceholder from '../../components/chat/NewChatComposerPlaceholder.svelte';
  import DeviceProvider from '../../device/DeviceProvider.svelte';
  import DeviceOnboarding from './DeviceOnboarding.svelte';
  import MobileBotChatSkeleton from '../../views/mobile/bots/MobileBotChatSkeleton.svelte';
  let { handle, children }: { handle?: string; children: Snippet } = $props()
  const host = useDashboardHost()
  let currentLocation = $state(window.location.pathname);
  const currentPath = $derived(dashboardRelativePath(currentLocation));
  // On phones the entry page and Bot chats open while the device connects: the entry page reopens the
  // last Bot chat at once, and a Bot chat shows its saved messages read-only until the device is online.
  const opensEarly = $derived(isMobileShell() && (currentPath === '/' || page.params.botId !== undefined))
  const device = $derived(host.activeDevice?.state === 'online' || (opensEarly && host.activeDevice?.state === 'connecting')
    ? host.activeDevice.runtime : null)
</script>

<svelte:window onpopstate={() => currentLocation = window.location.pathname} />

{#snippet statusAction()}
  {#if host.statusView?.mood === 'offline'}<Button variant="outline" onclick={host.retryActiveOrRefresh}>{$t('common.tryAgain')}</Button>{/if}
{/snippet}

{#snippet deviceSelector(disabled: boolean)}
  {#if host.canSwitchDevice}
    <ComposerDeviceSelect devices={host.devices} selected={host.activeDevice} onSelect={host.selectDevice} {disabled} />
  {/if}
{/snippet}

{#if currentPath === '/settings/shortcuts'}
  {@render children()}
{:else if device}
  {#key device}
    <DeviceProvider {device}>
      {#if handle && currentPath === '/new' && !isMobileShell()}
        <NewChatPage routeScope={host.routeScope} {deviceSelector} />
      {:else if handle}
        {@render children()}
      {:else if currentPath === '/bot'}
        <BotEntryPage routeScope={host.routeScope} />
      {:else}
        <DashboardEntryPage routeScope={host.routeScope} {deviceSelector} />
      {/if}
    </DeviceProvider>
  {/key}
{:else if host.signInUrl}
  <div class="flex min-h-96 flex-col items-center justify-center gap-3 bg-surface p-4" role="status">
    <SpinnerGapIcon class="size-6 animate-spin text-primary" aria-hidden="true" />
    <p class="text-sm text-muted-foreground">{$t('auth.redirecting')}</p>
    <Button size="sm" variant="outline" href={host.signInUrl}>{$t('auth.signIn')}</Button>
  </div>
{:else if host.statusView}
  {#if opensEarly && host.statusView.mood === 'loading'}
    <MobileBotChatSkeleton />
  {:else if currentPath === '/' && !isMobileShell()}
    <NewChatLayout status={host.statusView}>
      {#snippet action()}{@render statusAction()}{/snippet}
      <NewChatComposerPlaceholder {deviceSelector} />
    </NewChatLayout>
  {:else}
    <div class="flex min-h-[calc(100svh-5rem)] items-center justify-center py-12">
      <DeviceStatus status={host.statusView}>
        {#snippet action()}{@render statusAction()}{/snippet}
      </DeviceStatus>
    </div>
  {/if}
{:else if host.loadError}
  <div role="alert" class="flex items-center gap-3 py-4 text-sm">
    <span>{host.loadError}</span>
    <Button variant="outline" onclick={host.retryActiveOrRefresh}>{$t('common.tryAgain')}</Button>
  </div>
{:else if host.onboarding}
  <DeviceOnboarding checking={host.checkingDevices} onCheck={host.checkDevices} />
{/if}
