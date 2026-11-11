<script lang="ts" module>
  // The inline and fullscreen composers render this component for the same draft; notify once per error.
  const notified = new Map<string, string>()
</script>

<script lang="ts">
  import { toast } from 'svelte-sonner'
  import { Progress } from '$lib/components/ui/progress/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import FolderOpenIcon from 'phosphor-svelte/lib/FolderOpenIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { type UploadDraft, type UploadItem } from '$lib/uploads/draft'

  let { draft, disabled = false }: { draft: UploadDraft; disabled?: boolean } = $props()
  let picker: HTMLInputElement
  let resumeItem: UploadItem | undefined
  export function choose(item?: UploadItem) { resumeItem = item; picker.multiple = !item; picker.click() }
  $effect(() => {
    const { warning, items } = $draft
    notify('upload-warning', warning, () => toast.warning(warning!, { id: 'upload-warning' }))
    for (const item of items)
      notify(`upload-error-${item.id}`, item.error, () => toast.error(`Upload failed: ${item.name}`, { id: `upload-error-${item.id}`, description: item.error }))
  })
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
</script>

<svelte:window ononline={() => draft.reconnect()} />
<input bind:this={picker} type="file" class="hidden" aria-label="Select files to upload" onchange={() => void selected()} />
{#if $draft.items.length > 0}
<div class="px-3 pt-2">
  <ul class="flex flex-wrap gap-2">
    {#each $draft.items as item (item.id)}
      <li class="flex max-w-full flex-wrap items-center gap-1.5 rounded-md border border-border bg-background py-0.5 pl-2 pr-0.5 text-xs">
        <span class="max-w-64 truncate" title={item.name}>{item.name}</span>
        {#if item.status === 'complete'}<span>Uploaded File</span>
        {:else if item.status === 'hashing'}<span role="status">Checking file…</span>
        {:else if item.status === 'uploading'}<Progress class="w-28" max={item.size || 1} value={item.offset} aria-label={`Uploading ${item.name}`} />
        {:else if item.status === 'select'}<span>Select the original file to resume</span>
        {:else if item.status === 'failed'}<span role="img" class="text-destructive" aria-label={`Upload failed: ${item.name}`} title="Upload failed"><WarningCircleIcon class="size-3.5" weight="fill" /></span>{/if}
        {#if (item.status === 'select' || item.status === 'failed') && !item.recreate}
          <Button type="button" size="icon-sm" variant="ghost" {disabled} aria-label={`Retry upload ${item.name}`} title="Retry" onclick={() => item.file || (item.record && item.status === 'failed') ? void draft.retry(item) : choose(item)}><ArrowClockwiseIcon class="size-3.5" /></Button>
          {#if !item.file}<Button type="button" size="icon-sm" variant="ghost" {disabled} aria-label={`Select file for ${item.name}`} title="Select file" onclick={() => choose(item)}><FolderOpenIcon class="size-3.5" /></Button>{/if}
        {/if}
        <Button type="button" size="icon-sm" variant="ghost" {disabled} aria-label={`Remove attachment ${item.name}`} title="Remove" onclick={() => void draft.remove(item)}><XIcon class="size-3.5" /></Button>
      </li>
    {/each}
  </ul>
</div>
{/if}
