<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements'
  import { t } from '$lib/i18n'
  import FilesIcon from 'phosphor-svelte/lib/FilesIcon'
  import type { UploadedFile } from '$lib/uploads/message'
  import { fileKindOf } from '../fileKind'

  /**
   * Files a user uploaded with a sent message, in the shared file card style. Uploads live in Pontia's
   * state directory, which the device file API does not serve, so the card shows names and types only.
   */
  let { files, class: className, ...rest }: HTMLAttributes<HTMLDivElement> & { files: UploadedFile[] } = $props()
</script>

{#snippet row(file: UploadedFile)}
  {@const kind = fileKindOf(file.name)}
  {@const dot = file.name.lastIndexOf('.')}
  {@const extension = dot > 0 ? file.name.slice(dot + 1) : ''}
  <div class={['grid size-11 shrink-0 place-items-center rounded-item', kind.tone]}><kind.icon class="size-6" weight="duotone" /></div>
  <div class="min-w-0 flex-1" title={file.path}>
    <p class="flex min-w-0 text-sm font-medium text-heading">
      <span class="truncate">{dot > 0 ? file.name.slice(0, dot) : file.name}</span>{#if extension}<span class="shrink-0">.{extension}</span>{/if}
    </p>
    <p class="truncate text-xs text-muted-foreground">{extension ? extension.toUpperCase() : $t('files.file')}</p>
  </div>
{/snippet}

{#if files.length === 1}
  <div class={['ml-auto flex w-80 max-w-full items-center gap-3 rounded-panel bg-muted p-2.5', className]} data-user-message-uploads {...rest}>
    {@render row(files[0])}
  </div>
{:else}
  <div class={['ml-auto w-80 max-w-full rounded-panel bg-muted p-1', className]} data-user-message-uploads {...rest}>
    <div class="flex items-center gap-2 px-1.5 pt-0.5 pb-1">
      <FilesIcon class="size-4 shrink-0 text-muted-foreground" />
      <p class="min-w-0 flex-1 text-xs leading-4 text-muted-foreground tabular-nums">{$t('files.fileCount', { count: files.length })}</p>
    </div>
    <ul class="space-y-0.5">
      {#each files as file (file.path)}<li class="flex items-center gap-3 rounded-item p-1.5">{@render row(file)}</li>{/each}
    </ul>
  </div>
{/if}
