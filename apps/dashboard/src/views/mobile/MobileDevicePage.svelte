<script lang="ts">
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import DownloadIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { downloads } from '$lib/downloads/manager'
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { useDashboardHost } from '../../device/hostContext'
  import { settingsSections } from '../../components/settings/sections'
  import DownloadList from '../../components/layout/DownloadList.svelte'
  const host = useDashboardHost()
  let downloadsOpen = $state(false)
  $effect(() => { if (!$downloads.length) downloadsOpen = false })
</script>

<div class="my-3 flex items-center gap-4 rounded-2xl border bg-card p-4">
  <DesktopIcon class="size-8 text-primary" /><div><p class="font-semibold">{host.activeDevice?.name}</p><small>{$t(host.activeDevice?.state === 'online' ? 'devices.online' : host.connecting ? 'devices.connecting' : 'devices.offline')}</small></div>
</div>
{#each settingsSections as section (section.path)}
  <button class="mobile-row" onclick={() => navigate(section.path)}><section.icon />{$t(section.label)}<CaretRightIcon class="ml-auto text-muted-foreground" /></button>
{/each}
{#if $downloads.length}<button class="mobile-row" onclick={() => downloadsOpen = true}><DownloadIcon />{$t('downloads.title')}<CaretRightIcon class="ml-auto" /></button>{/if}

<Sheet.Root bind:open={downloadsOpen}>
  <BottomSheet dismiss={() => downloadsOpen = false} aria-describedby={undefined}>
    <Sheet.Title class="py-3">{$t('downloads.title')}</Sheet.Title>
    <DownloadList />
  </BottomSheet>
</Sheet.Root>
