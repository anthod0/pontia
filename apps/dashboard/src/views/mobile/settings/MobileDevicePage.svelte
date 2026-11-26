<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import DownloadIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { downloads } from '$lib/downloads/manager'
  import { useMobileHeader } from '../../../shell/mobile/header'
  import MobileDownloadsSheet from './MobileDownloadsSheet.svelte'
  import { useDashboardHost } from '../../../device/hostContext'
  import { settingsSections } from '../../../components/settings/sections'
  import { deviceStateLabel } from '../../../components/devices/deviceState'
  const { switchDevice } = useMobileHeader()
  const activeCount = $derived($downloads.filter((task) => task.status === 'receiving').length)
  const host = useDashboardHost()
  let downloadsOpen = $state(false)
  $effect(() => {
    if (!$downloads.length) downloadsOpen = false
  })
</script>

<div class="my-3 flex items-center gap-4 rounded-panel border bg-card p-4">
  <DesktopIcon class="size-8 shrink-0 text-primary" />
  <div class="min-w-0">
    <p class="truncate font-semibold">{host.activeDevice?.name}</p>
    <small>{$t(deviceStateLabel[host.activeDevice?.state ?? 'offline'])}</small>
  </div>
  {#if host.canSwitchDevice}<button
      class="ml-auto min-h-11 rounded-chip bg-muted px-4"
      onclick={switchDevice}>{$t('devices.switch')}</button
    >{/if}
</div>
<h2 class="mb-2 text-sm font-medium text-muted-foreground">{$t('settings.title')}</h2>
<nav class="divide-y overflow-hidden rounded-panel border bg-card px-3" aria-label={$t('settings.sections')}>
  {#each settingsSections.filter(section => !section.desktopOnly) as section (section.path)}
    <button class="mobile-row" onclick={() => navigate(section.path)}
      ><section.icon />{$t(section.label)}<CaretRightIcon class="ml-auto text-muted-foreground" /></button
    >
  {/each}
</nav>
{#if $downloads.length}<button
    class="mobile-row mt-4 rounded-panel border bg-card px-3"
    onclick={() => (downloadsOpen = true)}
    ><DownloadIcon />{$t('downloads.title')}{#if activeCount}<small
        >{$t('downloads.activeCount', { count: activeCount })}</small
      >{/if}<CaretRightIcon class="ml-auto" /></button
  >{/if}

<MobileDownloadsSheet bind:open={downloadsOpen} />
