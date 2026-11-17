<script lang="ts">
  import { t } from '$lib/i18n';
  import type { Snippet } from 'svelte'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import FilePlusIcon from 'phosphor-svelte/lib/FilePlusIcon'
  import FileMagnifyingGlassIcon from 'phosphor-svelte/lib/FileMagnifyingGlassIcon'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { cn } from '$lib/utils.js'
  import type { ManagedFileChange } from '../../../api/types'

  interface Props {
    operation: 'read' | 'write' | 'edit' | 'change'
    files: string[]
    changes?: ManagedFileChange[]
    results?: { file: string; content: string; error: boolean }[]
    connected?: boolean
    children?: Snippet
  }

  let { operation, files, changes = [], results = [], connected = false, children }: Props = $props()
  let open = $state(false)

  const Icon = $derived(operation === 'read' ? FileMagnifyingGlassIcon : operation === 'write' ? FilePlusIcon : FileTextIcon)
  const label = $derived($t('chat.fileCount', { operation: operation === 'read' ? $t('files.read') : operation === 'write' ? $t('files.write') : operation === 'change' ? $t('files.change') : $t('common.edit'), count: files.length }))
</script>

<Collapsible.Root bind:open class="relative min-w-0">
  <Collapsible.Trigger
    class="group/file-step flex w-full min-w-0 gap-3 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={open ? $t('chat.hideDetails', { title: label }) : $t('chat.showDetails', { title: label })}
  >
    <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
      <Icon class="size-4 group-hover/file-step:hidden" aria-hidden="true" />
      <CaretRightIcon class={cn('hidden size-4 transition-transform group-hover/file-step:block', open && 'rotate-90')} aria-hidden="true" />
    </span>
    <span class="min-w-0 flex-1 pt-0.5 text-sm font-medium leading-5 text-foreground/75">{label}</span>
  </Collapsible.Trigger>

  {#if connected}
    <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
  {/if}

  <Collapsible.Content>
    {#if changes.length}
      {#each changes as change, index (`${change.path}:${index}`)}
        <div class="mb-2 ml-9 min-w-0 text-sm text-muted-foreground">
          <div class="break-words py-1">
            {change.operation === 'add' ? $t('common.add') : change.operation === 'delete' ? $t('common.delete') : $t('common.update')} {change.path}{change.move_path ? ` → ${change.move_path}` : ''}
          </div>
          {#if change.diff}
            <pre class="max-w-full overflow-x-auto whitespace-pre-wrap [overflow-wrap:anywhere] px-2 py-1 font-mono text-xs leading-relaxed">{change.diff}</pre>
          {/if}
        </div>
      {/each}
    {:else}
      <ul class="mb-2 ml-9 space-y-1 py-1 text-sm leading-5 text-muted-foreground">
        {#each files as file (file)}
          <li class="break-words">{file}</li>
        {/each}
      </ul>
    {/if}
    {#each results as result, index (`${result.file}:${index}`)}
      <div class="mb-2 ml-9 max-w-[calc(100%_-_2.25rem)] overflow-x-auto">
        <div class="px-2 text-xs text-muted-foreground">{result.file}</div>
        <pre class={cn('whitespace-pre-wrap [overflow-wrap:anywhere] px-2 py-1 font-mono text-xs leading-relaxed text-muted-foreground', result.error && 'text-destructive')}>{result.content}</pre>
      </div>
    {/each}
    {@render children?.()}
  </Collapsible.Content>
</Collapsible.Root>
