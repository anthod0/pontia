<script lang="ts">
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import type { RecentFile } from '$lib/botRecentFiles'
  import { fileKindOf } from '../fileKind'

  /** Files opened before in a side panel, newest first, to open again. */
  let { files, onopen, onclear }: {
    files: RecentFile[]
    onopen: (file: RecentFile) => void
    onclear: () => void
  } = $props()
  const id = $props.id()
</script>

<section class="min-h-0 flex-1 overflow-auto overscroll-contain p-3" aria-labelledby={id}>
  <div class="flex items-center justify-between gap-2 pb-1 pl-2">
    <h2 {id} class="text-xs font-medium text-muted-foreground">{$t('files.recent')}</h2>
    <Button variant="ghost" size="xs" onclick={onclear}>{$t('files.clearRecent')}</Button>
  </div>
  <ul>
    {#each files as recent (recent.path)}
      {@const slash = recent.path.lastIndexOf('/')}
      {@const name = recent.path.slice(slash + 1)}
      {@const kind = fileKindOf(name)}
      <li>
        <button type="button" class="flex w-full items-center gap-3 rounded-item p-2 text-left outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
          title={recent.path} aria-label={$t('files.previewNamed', { name })} onclick={() => onopen(recent)}>
          <span class={['grid size-9 shrink-0 place-items-center rounded-item', kind.tone]}><kind.icon class="size-5" weight="duotone" /></span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm font-medium">{name}</span>
            {#if slash > 0}<span class="block truncate text-xs text-muted-foreground" dir="rtl"><bdi>{recent.path.slice(0, slash)}</bdi></span>{/if}
          </span>
        </button>
      </li>
    {/each}
  </ul>
</section>
