<script lang="ts">
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import DeviceMobileIcon from 'phosphor-svelte/lib/DeviceMobileIcon'
  import GlobeIcon from 'phosphor-svelte/lib/GlobeIcon'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { t } from '$lib/i18n'
  import { parseLocalhostPreviewTarget, type LocalhostPreviewTarget } from '$lib/localhostPreview'
  import type { ApiAccess } from '../../device/apiAccess'

  let { target = $bindable(), access }: { target: LocalhostPreviewTarget | null; access: ApiAccess } = $props()
  let address = $derived(target?.rawHref ?? '')
  let mobile = $state(false)
  let panelWidth = $state(0)
  const viewportWidth = $derived(mobile ? 375 : Math.max(1024, panelWidth))
  const scale = $derived(panelWidth ? Math.min(1, panelWidth / viewportWidth) : 1)
  let invalidAddress = $state<string | null>(null)
  let navigation = $state(0)

  function isDashboardPort(port: number) {
    const dashboardPort = location.port || (location.protocol === 'https:' ? '443' : '80')
    return port === Number(dashboardPort) || port === Number(import.meta.env.VITE_DEV_BACKEND_PORT)
  }

  function navigate(current: LocalhostPreviewTarget) {
    const url = URL.parse(address, current.rawHref)
    const next = parseLocalhostPreviewTarget(url?.href)
    if (!next || url?.origin !== new URL(current.rawHref).origin) {
      invalidAddress = address
      return
    }
    target = next
  }

  function load(frame: HTMLIFrameElement, current: LocalhostPreviewTarget) {
    if (access.openLocalhostPreview) access.openLocalhostPreview(current, frame)
    else frame.src = current.rawHref
  }

  function openTab(current: LocalhostPreviewTarget) {
    if (access.openLocalhostPreview) access.openLocalhostPreview(current)
    else window.open(current.rawHref, '_blank', 'noopener,noreferrer')
  }
</script>

{#if target && !access.openLocalhostPreview && isDashboardPort(target.port)}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Description role="alert">{$t('preview.selfPort', {port: target.port})}</Empty.Description>
  </Empty.Root>
{:else if target}
  {@const current = target}
  <div class="flex h-12 shrink-0 items-center gap-1 border-b px-2">
    <form class="min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); navigate(current) }}>
      <Input aria-label={$t('preview.address')} aria-invalid={invalidAddress === address} bind:value={address} oninput={() => invalidAddress = null} />
    </form>
    <Button size="icon-sm" variant="ghost" aria-label={$t('common.refresh')} title={$t('common.refresh')} onclick={() => navigation++}><ArrowClockwiseIcon /></Button>
    <Button size="icon-sm" variant="ghost" aria-label={$t('preview.mobile')} aria-pressed={mobile} title={$t('preview.mobile')} onclick={() => mobile = !mobile}>
      {#if mobile}<DeviceMobileIcon />{:else}<DesktopIcon />{/if}
    </Button>
    <Button size="icon-sm" variant="ghost" aria-label={$t('preview.newTab')} title={$t('preview.newTab')} onclick={() => openTab(current)}><ArrowSquareOutIcon /></Button>
  </div>
  <div bind:clientWidth={panelWidth} class="flex min-h-0 flex-1 justify-center overflow-hidden bg-muted">
    {#key [current, navigation]}
      <iframe use:load={current} title={$t('preview.open')} class="max-w-none origin-top shrink-0 border-0 bg-white"
        style:width={viewportWidth + 'px'} style:height={100 / scale + '%'} style:transform={'scale(' + scale + ')'} referrerpolicy="no-referrer"></iframe>
    {/key}
  </div>
{:else}
  <Empty.Root class="min-h-0 p-8">
    <Empty.Header>
      <Empty.Media class="size-12 rounded-item bg-muted text-muted-foreground"><GlobeIcon class="size-6" /></Empty.Media>
      <Empty.Title>{$t('preview.noneOpen')}</Empty.Title>
    </Empty.Header>
  </Empty.Root>
{/if}
