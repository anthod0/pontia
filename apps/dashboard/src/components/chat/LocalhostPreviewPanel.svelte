<script lang="ts">
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import DesktopIcon from 'phosphor-svelte/lib/DesktopIcon'
  import DeviceMobileIcon from 'phosphor-svelte/lib/DeviceMobileIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { t } from '$lib/i18n'
  import { parseLocalhostPreviewTarget, type LocalhostPreviewTarget } from '$lib/localhostPreview'
  import type { ApiAccess } from '../../device/apiAccess'

  let { target, access, onclose }: { target: LocalhostPreviewTarget; access: ApiAccess; onclose: () => void } = $props()
  // Each clicked link mounts a new panel.
  // svelte-ignore state_referenced_locally
  let current = $state.raw(target)
  // svelte-ignore state_referenced_locally
  let address = $state(current.rawHref)
  let mobile = $state(false)
  let panelWidth = $state(0)
  const viewportWidth = $derived(mobile ? 375 : Math.max(1024, panelWidth))
  const scale = $derived(panelWidth ? Math.min(1, panelWidth / viewportWidth) : 1)
  let invalid = $state(false)
  let navigation = $state(0)

  function navigate() {
    const url = URL.parse(address, current.rawHref)
    const next = parseLocalhostPreviewTarget(url?.href)
    if (!next || url?.origin !== new URL(target.rawHref).origin) {
      invalid = true
      return
    }
    current = next
    address = next.rawHref
    navigation++
  }

  function load(frame: HTMLIFrameElement) {
    if (access.openLocalhostPreview) access.openLocalhostPreview(current, frame)
    else frame.src = current.rawHref
  }

  function openTab() {
    if (access.openLocalhostPreview) access.openLocalhostPreview(current)
    else window.open(current.rawHref, '_blank', 'noopener,noreferrer')
  }
</script>

<div class="flex h-12 shrink-0 items-center gap-1 border-b px-2">
  <form class="min-w-0 flex-1" onsubmit={(event) => { event.preventDefault(); navigate() }}>
    <Input aria-label={$t('preview.address')} aria-invalid={invalid} bind:value={address} oninput={() => invalid = false} />
  </form>
  <Button size="icon-sm" variant="ghost" aria-label={$t('common.refresh')} title={$t('common.refresh')} onclick={() => navigation++}><ArrowClockwiseIcon /></Button>
  <Button size="icon-sm" variant="ghost" aria-label={$t('preview.mobile')} aria-pressed={mobile} title={$t('preview.mobile')} onclick={() => mobile = !mobile}>
    {#if mobile}<DeviceMobileIcon />{:else}<DesktopIcon />{/if}
  </Button>
  <Button size="icon-sm" variant="ghost" aria-label={$t('preview.newTab')} title={$t('preview.newTab')} onclick={openTab}><ArrowSquareOutIcon /></Button>
  <Button size="icon-sm" variant="ghost" aria-label={$t('common.close')} title={$t('common.close')} onclick={onclose}><XIcon /></Button>
</div>
<div bind:clientWidth={panelWidth} class="flex min-h-0 flex-1 justify-center overflow-hidden bg-muted">
  {#key navigation}
    <iframe use:load title={$t('preview.open')} class="max-w-none origin-top shrink-0 border-0 bg-white"
      style:width={viewportWidth + 'px'} style:height={100 / scale + '%'} style:transform={'scale(' + scale + ')'} referrerpolicy="no-referrer"></iframe>
  {/key}
</div>
