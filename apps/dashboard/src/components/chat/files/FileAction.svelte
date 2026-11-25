<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import { fileActionButton, fileActionClass, type FileActionLook } from './fileActionButton'
  import type { SharedFileState } from './sharedFileState.svelte'

  /** `overlay` draws the action over an image. */
  let { state, look = 'row' }: { state: SharedFileState; look?: FileActionLook } = $props()
  const overlay = $derived(look === 'overlay')
  // Progress is a larger ring centred on an image tile, and the size of the other actions in a row.
  const ring = $derived(overlay ? 'size-9' : 'size-8')
  const name = $derived(state.file.name)
</script>

{#if state.receiving}
  <button type="button" class={['inline-grid shrink-0 place-items-center rounded-full p-0 outline-none focus-visible:ring-2 focus-visible:ring-ring', ring, overlay ? 'bg-black/55 text-white backdrop-blur-sm' : 'text-muted-foreground hover:bg-background hover:text-foreground']} onclick={() => state.cancel()} aria-label={$t('downloads.cancelNamed', { name })} title={$t('common.cancel')}>
    <svg viewBox="0 0 36 36" class={['col-start-1 row-start-1 -rotate-90', ring]} aria-hidden="true">
      <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" class={overlay ? 'stroke-white/30' : 'stroke-border'} />
      {#if state.percent === null}
        <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class={['origin-center animate-spin', overlay ? 'stroke-white' : 'stroke-primary']} stroke-dasharray="25 100" />
      {:else}
        <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class={['transition-[stroke-dashoffset]', overlay ? 'stroke-white' : 'stroke-primary']} pathLength="100" stroke-dasharray="100" stroke-dashoffset={100 - state.percent} />
      {/if}
    </svg>
    <XIcon class="col-start-1 row-start-1 size-3.5" weight="bold" />
  </button>
{:else if !state.missing}
  <Button {...fileActionButton} class={fileActionClass[look]} onclick={() => state.download()} aria-label={state.failure ? $t('downloads.retryNamed', { name }) : $t('files.downloadNamed', { name })} title={state.failure ? $t('common.retry') : $t('files.download')}>
    {#if state.failure}<ArrowClockwiseIcon weight="bold" />{:else}<DownloadSimpleIcon weight="bold" />{/if}
  </Button>
{/if}
