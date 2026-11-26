<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import DownloadSimpleIcon from 'phosphor-svelte/lib/DownloadSimpleIcon'
  import type { SharedFileState } from './sharedFileState.svelte'

  /** Starts the usual download of every file in a share, skipping files known to be gone. */
  let { states }: { states: SharedFileState[] } = $props()
  const available = $derived(states.filter(state => !state.missing))
  function downloadAll() {
    for (const state of available) if (!state.receiving) state.download()
  }
</script>

<Button variant="outline" class="shrink-0 px-3 text-xs enabled:hover:border-primary-solid enabled:hover:bg-primary-solid enabled:hover:text-primary-foreground" disabled={!available.length} onclick={downloadAll} aria-label={$t('files.downloadAllNamed', { count: available.length })}>
  <DownloadSimpleIcon class="size-3.5" weight="bold" />{$t('files.downloadAll')}
</Button>
