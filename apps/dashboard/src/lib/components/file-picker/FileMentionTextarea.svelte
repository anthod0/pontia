<script lang="ts">
  import { tick } from 'svelte'
  import { File, Folder } from '@lucide/svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import { listWorkspaceFilePickerEntries } from '../../../api/client'
  import type { FilePickerFileView } from '../../../api/types'
  import { activeFileMention, replaceFileMention, type ActiveFileMention } from '../../file-picker/fileMention'

  interface Props {
    value: string
    workspaceId?: string | null
    disabled?: boolean
    placeholder?: string
    class?: string
    id?: string
    onkeydown?: (event: KeyboardEvent) => void
    onfocus?: (event: FocusEvent) => void
    shortcutFocusTarget?: boolean
  }

  let {
    value = $bindable(''),
    workspaceId = null,
    disabled = false,
    placeholder,
    class: className,
    id,
    onkeydown,
    onfocus,
    shortcutFocusTarget = false,
  }: Props = $props()

  let textarea = $state<HTMLTextAreaElement | null>(null)
  let listbox = $state<HTMLDivElement | null>(null)
  let mention = $state<ActiveFileMention | null>(null)
  let files = $state<FilePickerFileView[]>([])
  let open = $state(false)
  let loading = $state(false)
  let selectedIndex = $state(0)
  let debounce: ReturnType<typeof setTimeout> | null = null
  let controller: AbortController | null = null
  let requestId = 0

  function closePicker(): void {
    open = false
    mention = null
    files = []
    selectedIndex = 0
    if (debounce) clearTimeout(debounce)
    controller?.abort()
  }

  function sameMention(a: ActiveFileMention | null, b: ActiveFileMention | null): boolean {
    return a !== null && b !== null && a.start === b.start && a.end === b.end && a.query === b.query
  }

  function refreshMention(): void {
    if (!textarea || disabled || !workspaceId) {
      closePicker()
      return
    }
    const current = activeFileMention(value, textarea.selectionStart ?? value.length)
    const previous = mention
    mention = current
    open = current !== null
    if (!current) {
      files = []
      selectedIndex = 0
      return
    }
    if (sameMention(previous, current)) return
    selectedIndex = 0
    scheduleSearch(current.query)
  }

  function scheduleSearch(query: string): void {
    if (debounce) clearTimeout(debounce)
    debounce = setTimeout(() => void search(query), 150)
  }

  async function search(query: string): Promise<void> {
    if (!workspaceId) return
    controller?.abort()
    const localController = new AbortController()
    controller = localController
    const localRequestId = ++requestId
    loading = true
    try {
      const result = await listWorkspaceFilePickerEntries(workspaceId, query, { limit: 20, signal: localController.signal })
      if (localRequestId !== requestId) return
      files = result.files
      selectedIndex = 0
      open = mention !== null
    } catch (error) {
      if ((error as Error).name !== 'AbortError') files = []
    } finally {
      if (localRequestId === requestId) loading = false
    }
  }

  async function choose(file: FilePickerFileView): Promise<void> {
    if (!mention) return
    const next = replaceFileMention(value, mention, file.path)
    value = next.value
    closePicker()
    await tick()
    textarea?.focus()
    textarea?.setSelectionRange(next.cursor, next.cursor)
  }

  function isPickerControlKey(key: string): boolean {
    return key === 'ArrowDown' || key === 'ArrowUp' || key === 'Enter' || key === 'Tab' || key === 'Escape'
  }

  async function scrollSelectedIntoView(): Promise<void> {
    await tick()
    listbox
      ?.querySelector(`[data-file-suggestion-index="${selectedIndex}"]`)
      ?.scrollIntoView({ block: 'nearest' })
  }

  function moveSelection(delta: number): void {
    selectedIndex = files.length ? (selectedIndex + delta + files.length) % files.length : 0
    void scrollSelectedIntoView()
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (open) {
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        moveSelection(1)
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        moveSelection(-1)
        return
      }
      if ((event.key === 'Enter' || event.key === 'Tab') && files[selectedIndex]) {
        event.preventDefault()
        void choose(files[selectedIndex])
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        closePicker()
        return
      }
    }
    onkeydown?.(event)
  }

  function handleKeyup(event: KeyboardEvent): void {
    if (isPickerControlKey(event.key)) return
    refreshMention()
  }
</script>

<div class="relative w-full">
  <PromptInput.Textarea
    {id}
    bind:ref={textarea}
    bind:value
    {placeholder}
    {disabled}
    data-chat-shortcut-focus-target={shortcutFocusTarget ? 'true' : undefined}
    class={className}
    onkeydown={handleKeydown}
    oninput={refreshMention}
    onkeyup={handleKeyup}
    onclick={refreshMention}
    onfocus={(event) => { onfocus?.(event); refreshMention() }}
  />

  {#if open}
    <div bind:this={listbox} class="absolute bottom-full left-2 z-50 mb-2 max-h-64 w-[min(36rem,calc(100vw-3rem))] overflow-auto rounded-lg border bg-popover p-1 text-popover-foreground shadow-lg" role="listbox" aria-label="File suggestions">
      {#if loading && files.length === 0}
        <div class="px-3 py-2 text-sm text-muted-foreground">Searching files…</div>
      {:else if files.length === 0}
        <div class="px-3 py-2 text-sm text-muted-foreground">No matching files</div>
      {:else}
        {#each files as file, index (file.path)}
          <button
            type="button"
            class={`flex w-full min-w-0 items-center gap-2 rounded-md px-3 py-2 text-left text-sm ${index === selectedIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-accent hover:text-accent-foreground'}`}
            role="option"
            aria-selected={index === selectedIndex}
            data-file-suggestion-index={index}
            onmouseenter={() => (selectedIndex = index)}
            onmousedown={(event) => { event.preventDefault(); void choose(file) }}
          >
            {#if file.kind === 'directory'}
              <Folder class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            {:else}
              <File class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            {/if}
            <span class="min-w-0 truncate">@{file.path}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>
