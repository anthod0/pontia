<script lang="ts">
  import { ChevronRight, FilePenLine, FilePlus2, FileSearch } from '@lucide/svelte'
  import * as Collapsible from '$lib/components/ui/collapsible/index.js'
  import { cn } from '$lib/utils.js'

  interface Props {
    operation: 'read' | 'write' | 'edit'
    files: string[]
    connected?: boolean
  }

  let { operation, files, connected = false }: Props = $props()
  let open = $state(false)

  const Icon = $derived(operation === 'read' ? FileSearch : operation === 'write' ? FilePlus2 : FilePenLine)
  const label = $derived(`${operation === 'read' ? 'Read' : operation === 'write' ? 'Write' : 'Edit'} ${files.length} ${files.length === 1 ? 'file' : 'files'}`)
</script>

<Collapsible.Root bind:open class="relative min-w-0">
  <Collapsible.Trigger
    class="group/file-step flex w-full min-w-0 gap-3 py-1.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={open ? `Hide ${label} details` : `Show ${label} details`}
  >
    <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
      <Icon class="size-4 group-hover/file-step:hidden" aria-hidden="true" />
      <ChevronRight class={cn('hidden size-4 transition-transform group-hover/file-step:block', open && 'rotate-90')} aria-hidden="true" />
    </span>
    <span class="min-w-0 flex-1 pt-0.5 text-sm font-medium leading-5 text-foreground/75">{label}</span>
  </Collapsible.Trigger>

  {#if connected}
    <span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>
  {/if}

  <Collapsible.Content>
    <ul class="mb-2 ml-9 space-y-1 py-1 text-sm leading-5 text-muted-foreground">
      {#each files as file (file)}
        <li class="break-words">{file}</li>
      {/each}
    </ul>
  </Collapsible.Content>
</Collapsible.Root>
