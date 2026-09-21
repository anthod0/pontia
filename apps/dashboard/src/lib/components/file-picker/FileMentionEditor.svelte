<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { Editor } from '@tiptap/core'
  import Document from '@tiptap/extension-document'
  import HardBreak from '@tiptap/extension-hard-break'
  import Paragraph from '@tiptap/extension-paragraph'
  import Placeholder from '@tiptap/extension-placeholder'
  import Text from '@tiptap/extension-text'
  import { PluginKey } from '@tiptap/pm/state'
  import { exitSuggestion, type SuggestionProps } from '@tiptap/suggestion'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import { listWorkspaceFilePickerEntries } from '../../../api/client'
  import type { FilePickerFileView } from '../../../api/types'
  import { cn } from '$lib/utils.js'
  import { promptDocumentFromText, promptTextFromDocument } from '../../file-picker/fileMention'
  import { createFileMentionExtension } from '../../file-picker/fileMentionExtension'

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
    autofocus?: boolean
    mentionIdentities?: Map<string, FilePickerFileView>
    suggestionListId?: string
    activeSuggestionId?: string
  }

  let {
    value = $bindable(''),
    workspaceId = null,
    disabled = false,
    placeholder = '',
    class: className,
    id,
    onkeydown,
    onfocus,
    shortcutFocusTarget = false,
    autofocus = false,
    mentionIdentities = new Map(),
    suggestionListId,
    activeSuggestionId,
  }: Props = $props()

  const suggestionPluginKey = new PluginKey('fileMentionSuggestion')

  let editorElement = $state<HTMLDivElement | null>(null)
  let listbox = $state<HTMLDivElement | null>(null)
  let editorState = $state<{ editor: Editor | null }>({ editor: null })
  let files = $state<FilePickerFileView[]>([])
  let open = $state(false)
  let loading = $state(false)
  let selectedIndex = $state(0)
  let currentQuery = $state('')
  let chooseSuggestion: ((file: FilePickerFileView) => void) | null = null
  let autofocusHandled = false
  let initialFocusHandled = false

  export function focusEnd(): void {
    editorState.editor?.commands.focus('end', { scrollIntoView: false })
  }

  function closePicker(): void {
    open = false
    loading = false
    files = []
    selectedIndex = 0
    currentQuery = ''
    chooseSuggestion = null
  }

  function updatePicker(props: SuggestionProps): void {
    if (props.query !== currentQuery) selectedIndex = 0
    currentQuery = props.query
    files = props.items as FilePickerFileView[]
    loading = props.loading
    chooseSuggestion = (file) => props.command(file)
    open = true
  }

  function dismissPicker(): void {
    const editor = editorState.editor
    if (editor) exitSuggestion(editor.view, suggestionPluginKey)
    closePicker()
  }

  async function choose(file: FilePickerFileView): Promise<void> {
    chooseSuggestion?.(file)
    await tick()
    editorState.editor?.commands.focus()
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

  function deleteAdjacentMention(): boolean {
    const editor = editorState.editor
    if (!editor) return false
    const { selection } = editor.state
    if (!selection.empty) return false

    const directNode = selection.$from.nodeBefore
    if (directNode?.type.name === 'mention') {
      return editor.commands.deleteRange({ from: selection.from - directNode.nodeSize, to: selection.from })
    }
    if (!directNode?.isText || directNode.text !== ' ') return false

    const beforeSpace = selection.from - directNode.nodeSize
    const mention = editor.state.doc.resolve(beforeSpace).nodeBefore
    if (mention?.type.name !== 'mention') return false
    return editor.commands.deleteRange({ from: beforeSpace - mention.nodeSize, to: selection.from })
  }

  function handleEditorKeyDown(_view: unknown, event: KeyboardEvent): boolean {
    if (event.key === 'Backspace' && deleteAdjacentMention()) {
      event.preventDefault()
      return true
    }
    if (open) {
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        moveSelection(1)
        return true
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        moveSelection(-1)
        return true
      }
      if ((event.key === 'Enter' || event.key === 'Tab') && files[selectedIndex]) {
        event.preventDefault()
        void choose(files[selectedIndex])
        return true
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        dismissPicker()
        return true
      }
    }

    const previousValue = value
    onkeydown?.(event)
    if (event.defaultPrevented && value !== previousValue) {
      queueMicrotask(() => editorState.editor?.commands.focus('end'))
    }
    return event.defaultPrevented
  }

  onMount(() => {
    if (!editorElement) return

    const editor = new Editor({
      element: editorElement,
      extensions: [
        Document,
        Paragraph,
        Text,
        HardBreak,
        Placeholder.configure({ placeholder }),
        createFileMentionExtension({
          pluginKey: suggestionPluginKey,
          identities: mentionIdentities,
          enabled: () => Boolean(workspaceId && !disabled),
          search: async (query, signal) => {
            if (!workspaceId) return []
            const result = await listWorkspaceFilePickerEntries(workspaceId, query, { limit: 20, signal })
            return result.files
          },
          onSuggestion: updatePicker,
          onSuggestionExit: closePicker,
        }),
      ],
      content: promptDocumentFromText(value, mentionIdentities),
      editable: !disabled,
      editorProps: {
        attributes: {
          ...(id ? { id } : {}),
          role: 'textbox',
          'aria-multiline': 'true',
          'aria-placeholder': placeholder,
          placeholder,
          ...(shortcutFocusTarget ? { 'data-chat-shortcut-focus-target': 'true' } : {}),
          class: cn(
            'tiptap-prompt block max-h-48 min-h-10 w-full overflow-y-auto border-0 bg-transparent px-2 py-2 text-sm outline-none focus-visible:ring-0',
            className,
          ),
        },
        handleKeyDown: handleEditorKeyDown,
        handleDOMEvents: {
          focus: (_view, event) => {
            if (autofocus && !initialFocusHandled) {
              initialFocusHandled = true
              queueMicrotask(focusEnd)
            }
            onfocus?.(event as FocusEvent)
            return false
          },
        },
      },
      onUpdate: ({ editor }) => {
        const nextValue = promptTextFromDocument(editor.getJSON())
        if (nextValue !== value) value = nextValue
      },
    })

    editorState.editor = editor
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)

    return () => {
      editor.destroy()
      editorState.editor = null
    }
  })

  $effect(() => {
    const editor = editorState.editor
    if (!editor) return
    for (const [name, value] of [['aria-controls', suggestionListId], ['aria-activedescendant', activeSuggestionId]] as const) {
      if (value) editor.view.dom.setAttribute(name, value)
      else editor.view.dom.removeAttribute(name)
    }
  })

  $effect(() => {
    const editor = editorState.editor
    if (!editor) return
    editor.setEditable(!disabled, false)
    editor.view.dom.setAttribute('aria-disabled', String(disabled))
    if (disabled || !workspaceId) dismissPicker()
    if (autofocus && !autofocusHandled && !disabled) {
      autofocusHandled = true
      focusEnd()
    }
  })

  $effect(() => {
    const editor = editorState.editor
    if (!editor) return
    if (promptTextFromDocument(editor.getJSON()) === value) return
    editor.commands.setContent(promptDocumentFromText(value, mentionIdentities), { emitUpdate: false })
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)
  })
</script>

<div class="relative w-full">
  <div bind:this={editorElement}></div>

  {#if open}
    <div bind:this={listbox} class="absolute bottom-full left-2 z-50 mb-2 max-h-64 w-[min(36rem,calc(100vw-3rem))] overflow-auto rounded-none border bg-popover p-1 text-popover-foreground shadow-none" role="listbox" aria-label="File suggestions">
      {#if loading && files.length === 0}
        <div class="px-3 py-2 text-sm text-muted-foreground">Searching files…</div>
      {:else if files.length === 0}
        <div class="px-3 py-2 text-sm text-muted-foreground">No matching files</div>
      {:else}
        {#each files as file, index (file.path)}
          <button
            type="button"
            class={`flex w-full min-w-0 items-center gap-2 rounded-none px-3 py-2 text-left text-sm ${index === selectedIndex ? 'bg-accent text-accent-foreground' : 'hover:bg-accent hover:text-accent-foreground'}`}
            role="option"
            aria-selected={index === selectedIndex}
            data-file-suggestion-index={index}
            onmouseenter={() => (selectedIndex = index)}
            onmousedown={(event) => { event.preventDefault(); void choose(file) }}
          >
            {#if file.kind === 'directory'}
              <FolderIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            {:else}
              <FileIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            {/if}
            <span class="min-w-0 truncate">@{file.path}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(.tiptap-prompt p) {
    margin: 0;
  }

  :global(.tiptap-prompt p.is-editor-empty:first-child::before) {
    color: var(--muted-foreground);
    content: attr(data-placeholder);
    float: left;
    height: 0;
    pointer-events: none;
  }

  :global(.tiptap-prompt[aria-disabled='true']) {
    cursor: not-allowed;
    opacity: 0.5;
  }
</style>
