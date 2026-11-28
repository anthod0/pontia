<script lang="ts" module>
  import { t } from '$lib/i18n';
  // The inline and fullscreen composers render this component for the same draft; notify once per error.
  const notified = new Map<string, string>()
</script>

<script lang="ts">
  import SecretChip from './SecretChip.svelte'
  import { fileKind, fileKindOf } from './fileKind'
  import { toast } from 'svelte-sonner'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import FolderOpenIcon from 'phosphor-svelte/lib/FolderOpenIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { type UploadDraft, type UploadItem } from '$lib/uploads/draft'

  let { draft, disabled = false }: { draft: UploadDraft; disabled?: boolean } = $props()
  let picker: HTMLInputElement
  let resumeItem: UploadItem | undefined
  const unknownKind = fileKind('')
  export function choose(item?: UploadItem) { resumeItem = item; picker.multiple = !item; picker.click() }
  $effect(() => {
    const { warning, items } = $draft
    if (!isMobileShell()) {
      notify('upload-warning', warning, () => toast.warning(warning!, { id: 'upload-warning', duration: 6000 }))
      for (const item of items)
        notify(`upload-error-${item.id}`, item.error, () => toast.error($t('files.uploadFailedNamed', { name: displayName(item) }), { id: `upload-error-${item.id}`, description: item.error, duration: 6000 }))
    }
  })
  function displayName(item: UploadItem) {
    return item.secret?.label ?? item.secret?.masked ?? item.name
  }
  function extension(name: string) {
    const dot = name.lastIndexOf('.')
    return dot > 0 ? name.slice(dot + 1) : ''
  }
  function percent(item: UploadItem) {
    return item.size ? Math.floor((item.offset / item.size) * 100) : 0
  }
  function formatSize(bytes: number) {
    const units = ['B', 'KB', 'MB', 'GB']
    let value = bytes
    let unit = 0
    while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++ }
    return `${unit === 0 ? value : value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`
  }
  function statusText(item: UploadItem) {
    if (item.status === 'complete') return $t('files.uploaded')
    if (item.status === 'hashing') return $t('files.checking')
    if (item.status === 'uploading') return `${percent(item)}%`
    if (item.status === 'select') return $t('files.resumeHint')
    return item.error ? `${$t('files.uploadFailed')}: ${item.error}` : $t('files.uploadFailed')
  }
  function tooltip(item: UploadItem) {
    if (item.secret) return `${[item.secret.label, item.secret.masked].filter(Boolean).join(' · ')}\n${statusText(item)}`
    return `${displayName(item)}\n${statusText(item)} · ${formatSize(item.size)}`
  }
  function retry(item: UploadItem) {
    if (item.secret || item.file || (item.record && item.status === 'failed')) void draft.retry(item)
    else choose(item)
  }
  function notify(key: string, message: string | null | undefined, show: () => void) {
    if (!message) { notified.delete(key); return }
    if (notified.get(key) === message) return
    notified.set(key, message)
    show()
  }
  async function selected() {
    const files = Array.from(picker.files ?? [])
    picker.value = ''
    if (resumeItem && files[0]) await draft.retry(resumeItem, files[0])
    else await draft.add(files)
    resumeItem = undefined
  }
  const overlayButton = 'size-7 rounded-full bg-background/90 text-foreground shadow-sm hover:bg-background'
</script>

<svelte:window ononline={() => draft.reconnect()} />
<input bind:this={picker} type="file" class="hidden" aria-label={$t('files.selectUpload')} onchange={() => void selected()} />
{#if isMobileShell() && $draft.warning}<p role="status" class="px-3 pb-1 text-13 text-muted-foreground">{$draft.warning}</p>{/if}
{#if $draft.items.length > 0}
  <ul class="flex gap-3 px-3 pb-1 pt-3 max-sm:overflow-x-auto max-sm:[scrollbar-width:none] sm:flex-wrap">
    {#each $draft.items as item (item.id)}
      {@const kind = fileKindOf(item.name)}
      {@const thumbnail = item.thumbnailUrl}
      {@const failed = item.status === 'failed'}
      <li class="relative size-16 shrink-0" title={tooltip(item)}>
        <div class={['relative size-full overflow-hidden rounded-item', failed ? 'bg-destructive/10' : 'bg-muted']}>
          {#if item.secret}
            <SecretChip secret={item.secret} {failed} />
          {:else if thumbnail}
            <img src={thumbnail} alt={item.name} class="size-full object-cover" draggable="false" />
          {:else}
            {@const Icon = kind.icon}
            {@const ext = extension(item.name)}
            <div class={['flex size-full flex-col items-center justify-center gap-1', failed ? 'text-destructive' : kind === unknownKind ? 'text-muted-foreground' : kind.tone]}>
              <Icon class="size-6" />
              {#if ext}<span class="max-w-full truncate px-1 text-10 font-semibold uppercase leading-none tracking-wide">{ext}</span>{/if}
            </div>
          {/if}
          {#if item.status === 'hashing'}
            <div class="absolute inset-0 grid place-items-center bg-background/60 backdrop-blur-[1px]">
              <CircleNotchIcon class="size-5 animate-spin text-foreground" role="status" aria-label={$t('files.checking')} />
            </div>
          {:else if item.status === 'uploading'}
            {@const value = percent(item)}
            <div class="absolute inset-0 grid place-items-center bg-background/60 backdrop-blur-[1px]" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={value} aria-label={$t('files.uploading', { name: displayName(item) })}>
              <svg viewBox="0 0 36 36" class="col-start-1 row-start-1 size-9 -rotate-90" aria-hidden="true">
                <circle cx="18" cy="18" r="15" fill="none" stroke-width="3" class="stroke-foreground/15" />
                <circle cx="18" cy="18" r="15" fill="none" stroke-width="3" stroke-linecap="round" class="stroke-primary transition-[stroke-dashoffset]" pathLength="100" stroke-dasharray="100" stroke-dashoffset={100 - value} />
              </svg>
              <span class="col-start-1 row-start-1 text-10 font-semibold tabular-nums leading-none">{value}%</span>
            </div>
          {:else if item.status === 'select' || failed}
            <div class={['absolute inset-0 flex items-center justify-center gap-1 rounded-item', failed ? 'bg-destructive/15 ring-1 ring-inset ring-destructive/70' : 'bg-background/60']}>
              {#if !item.recreate}
                <Button type="button" size="icon-sm" variant="ghost" class={overlayButton} {disabled} aria-label={$t('files.retryUpload', { name: displayName(item) })} title={$t('common.retry')} onclick={() => retry(item)}><ArrowClockwiseIcon class="size-3.5" weight="bold" /></Button>
                {#if !item.secret && !item.file}<Button type="button" size="icon-sm" variant="ghost" class={overlayButton} {disabled} aria-label={$t('files.selectNamed', { name: displayName(item) })} title={$t('files.select')} onclick={() => choose(item)}><FolderOpenIcon class="size-3.5" weight="bold" /></Button>{/if}
              {/if}
            </div>
            {#if failed}
              <span role="img" class={['absolute grid place-items-center rounded-full bg-background text-destructive', item.recreate ? 'inset-0 m-auto size-6' : 'bottom-1 left-1 size-4']} aria-label={$t('files.uploadFailedNamed', { name: displayName(item) })}><WarningCircleIcon class="size-full" weight="fill" /></span>
            {/if}
          {/if}
        </div>
        <button type="button" class="absolute -right-1.5 -top-1.5 z-10 grid size-5 place-items-center rounded-full bg-neutral-900/85 text-white shadow-sm ring-2 ring-background outline-none transition-colors before:absolute before:-inset-2 hover:bg-neutral-900 focus-visible:ring-ring dark:bg-neutral-950/90 dark:ring-white/25 dark:hover:bg-neutral-950 disabled:pointer-events-none disabled:opacity-50" {disabled} aria-label={$t('files.removeAttachment', { name: displayName(item) })} title={$t('common.remove')} onclick={() => void draft.remove(item)}><XIcon class="size-3" weight="bold" /></button>
      </li>
    {/each}
  </ul>
{/if}
