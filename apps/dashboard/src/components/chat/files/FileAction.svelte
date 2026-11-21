<script lang="ts">
  import { t } from '$lib/i18n'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import type { SharedFileState } from './sharedFileState.svelte'

  /** `overlay` draws the button over an image; `small` fits an image tile corner. */
  let { state, overlay = false, small = false }: { state: SharedFileState; overlay?: boolean; small?: boolean } = $props()
  const box = $derived(small ? 'size-7' : 'size-9')
  const name = $derived(state.file.name)
</script>

{#if state.receiving}
  <button type="button" class={['relative grid shrink-0 place-items-center rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring', box, overlay ? 'bg-black/55 text-white backdrop-blur-sm' : 'text-muted-foreground hover:text-foreground']} onclick={() => state.cancel()} aria-label={$t('downloads.cancelNamed', { name })} title={$t('common.cancel')}>
    <svg viewBox="0 0 36 36" class={['absolute inset-0 -rotate-90', box]} aria-hidden="true">
      <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" class={overlay ? 'stroke-white/30' : 'stroke-border'} />
      {#if state.percent === null}
        <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class={['origin-center animate-spin', overlay ? 'stroke-white' : 'stroke-primary']} stroke-dasharray="25 100" />
      {:else}
        <circle cx="18" cy="18" r="16" fill="none" stroke-width="2.5" stroke-linecap="round" class={['transition-[stroke-dashoffset]', overlay ? 'stroke-white' : 'stroke-primary']} pathLength="100" stroke-dasharray="100" stroke-dashoffset={100 - state.percent} />
      {/if}
    </svg>
    <XIcon class={small ? 'size-3' : 'size-3.5'} weight="bold" />
  </button>
{:else if !state.missing}
  <button type="button" class={['grid shrink-0 place-items-center rounded-full outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring', box, overlay ? 'bg-black/55 text-white backdrop-blur-sm hover:bg-black/70' : 'bg-background text-foreground ring-1 ring-border hover:bg-primary-solid hover:text-primary-foreground hover:ring-primary-solid']} onclick={() => state.download()} aria-label={state.failure ? $t('downloads.retryNamed', { name }) : $t('files.downloadNamed', { name })} title={state.failure ? $t('common.retry') : $t('files.download')}>
    {#if state.failure}<ArrowClockwiseIcon class={small ? 'size-3.5' : 'size-4'} />{:else}<DownloadSimpleIcon class={small ? 'size-3.5' : 'size-4'} weight="bold" />{/if}
  </button>
{/if}
