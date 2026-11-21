<script lang="ts">
  import { t } from '$lib/i18n'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import type { SharedFileState } from './sharedFileState.svelte'

  /** Starts the usual download of every file in a share, skipping files known to be gone. */
  let { states }: { states: SharedFileState[] } = $props()
  const available = $derived(states.filter(state => !state.missing))
  function downloadAll() {
    for (const state of available) if (!state.receiving) state.download()
  }
</script>

<button type="button" class="flex h-8 shrink-0 items-center gap-1.5 rounded-full bg-background px-3 text-xs font-medium text-foreground ring-1 ring-border outline-none transition-colors enabled:hover:bg-primary-solid enabled:hover:text-primary-foreground enabled:hover:ring-primary-solid focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50" disabled={!available.length} onclick={downloadAll} aria-label={$t('files.downloadAllNamed', { count: available.length })}>
  <DownloadSimpleIcon class="size-3.5" weight="bold" />{$t('files.downloadAll')}
</button>
