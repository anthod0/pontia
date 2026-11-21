<script lang="ts">
  import { t, number } from '$lib/i18n'
  import { formatFileSize, type SharedFileState } from './sharedFileState.svelte'

  let { state }: { state: SharedFileState } = $props()
  const size = (value: number) => formatFileSize(value, $number)
</script>

<p class={['flex min-w-0 text-sm font-medium text-heading', state.missing && 'text-muted-foreground line-through decoration-muted-foreground/50']} title={state.file.name}>
  <span class="truncate">{state.stem}</span>{#if state.extension}<span class="shrink-0">.{state.extension}</span>{/if}
</p>
{#if state.progress}
  <p class="truncate text-xs text-muted-foreground tabular-nums" role="status">
    {$t('files.receiving')} {state.progress.total === null ? size(state.progress.received) : `${size(state.progress.received)} / ${size(state.progress.total)}`}
  </p>
{:else if state.failure}
  <p class={['text-xs [overflow-wrap:anywhere]', state.missing ? 'text-muted-foreground' : 'text-destructive']} role="alert">{state.failure}</p>
{:else}
  <p class="truncate text-xs text-muted-foreground tabular-nums">{state.extension ? state.extension.toUpperCase() : $t('files.file')} · {size(state.file.size)}</p>
{/if}
