<script lang="ts">
  import { Progress } from '$lib/components/ui/progress/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { type createUploadDraft, type UploadItem } from '$lib/uploads/draft'

  let { draft, disabled = false }: { draft: ReturnType<typeof createUploadDraft>; disabled?: boolean } = $props()
  let picker: HTMLInputElement
  let resumeItem: UploadItem | undefined
  export function choose(item?: UploadItem) { resumeItem = item; picker.multiple = !item; picker.click() }
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
{#if $draft.warning || $draft.items.length > 0}
<div class="px-3 pb-2">
  {#if $draft.warning}<p role="alert" class="text-xs text-destructive">{$draft.warning}</p>{/if}
  <ul class="space-y-1">
    {#each $draft.items as item (item.id)}
      <li class="flex flex-wrap items-center gap-2 text-xs">
        <span class="max-w-64 truncate" title={item.name}>{item.name}</span>
        {#if item.status === 'complete'}<span>Uploaded File</span>
        {:else if item.status === 'hashing'}<span role="status">Checking file…</span>
        {:else if item.status === 'uploading'}<Progress class="w-28" max={item.size || 1} value={item.offset} aria-label={`Uploading ${item.name}`} />
        {:else if item.status === 'select'}<span>Select the original file to resume</span>{/if}
        {#if item.error}<span role="alert" class="text-destructive">{item.error}</span>{/if}
        {#if (item.status === 'select' || item.status === 'failed') && !item.recreate}
          <Button type="button" size="sm" variant="ghost" {disabled} onclick={() => item.file || (item.record && item.status === 'failed') ? void draft.retry(item) : choose(item)}>Retry</Button>
          {#if !item.file}<Button type="button" size="sm" variant="ghost" {disabled} onclick={() => choose(item)}>Select file</Button>{/if}
        {/if}
        <Button type="button" size="sm" variant="ghost" {disabled} aria-label={`Remove attachment ${item.name}`} onclick={() => void draft.remove(item)}>Remove</Button>
      </li>
    {/each}
  </ul>
</div>
{/if}
