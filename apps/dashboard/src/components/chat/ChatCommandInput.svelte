<script lang="ts">
  import { tick, type ComponentProps } from 'svelte'
  import FileMentionEditor from '$lib/components/file-picker/FileMentionEditor.svelte'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { PluginKey } from '@tiptap/pm/state'
  import { exitSuggestion } from '@tiptap/suggestion'
  import type { ChatCommand } from '$lib/chatCommands'
  import { createChatCommandExtension, type ChatCommandSuggestion } from '$lib/chatCommandExtension'

  let {
    value = $bindable(''),
    commands,
    onCommand,
    onfocus,
    disabled = false,
    commandSide = 'top',
    ...editorProps
  }: ComponentProps<typeof FileMentionEditor> & {
    commands: ChatCommand[]
    onCommand: (command: ChatCommand) => void
    commandSide?: 'top' | 'bottom'
  } = $props()

  const componentId = $props.id()
  const listId = `${componentId}-commands`
  let anchor = $state<HTMLDivElement | null>(null)
  let editor = $state<FileMentionEditor | null>(null)
  let focused = $state(false)
  const pluginKey = new PluginKey('chatCommandSuggestion')
  let suggestion = $state.raw<ChatCommandSuggestion | null>(null)
  let selectedIndex = $state(0)
  const query = $derived(suggestion?.query)
  const matches = $derived(query === undefined ? [] : commands.filter((command) => command.name.startsWith(`/${query}`)))
  const open = $derived(focused && !disabled && suggestion !== null && matches.length > 0)
  const extension = createChatCommandExtension({
    pluginKey,
    commands: () => commands,
    onCommand: (command) => onCommand(command),
    render: () => ({
      onStart: updateSuggestion,
      onUpdate: updateSuggestion,
      onExit: () => { suggestion = null },
      onKeyDown: ({ event }) => handleKeydown(event),
    }),
  })

  function updateSuggestion(next: ChatCommandSuggestion): void {
    if (next.query !== suggestion?.query) selectedIndex = 0
    suggestion = next
  }

  function dismiss(): void {
    if (suggestion) exitSuggestion(suggestion.editor.view, pluginKey)
  }

  export function focusEnd(): void {
    editor?.focusEnd()
  }

  function choose(command: ChatCommand, execute = true): void {
    if (!open || disabled || command.disabledReason) return
    suggestion?.command({ command, execute })
  }

  function handleKeydown(event: KeyboardEvent): boolean {
    if (!open || event.isComposing || event.keyCode === 229 || event.shiftKey || event.ctrlKey || event.metaKey || event.altKey) return false
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      selectedIndex = (selectedIndex + (event.key === 'ArrowDown' ? 1 : -1) + matches.length) % matches.length
      void tick().then(() => document.getElementById(`${listId}-${selectedIndex}`)?.scrollIntoView({ block: 'nearest' }))
      return true
    }
    const command = matches[selectedIndex]
    if (command && (event.key === 'Enter' || event.key === 'Tab')) {
      choose(command, event.key === 'Enter')
      return true
    }
    return false
  }
</script>

<div bind:this={anchor} onfocusout={(event) => {
  if (!anchor?.contains(event.relatedTarget as Node | null)) focused = false
}}>
  <FileMentionEditor
    bind:this={editor}
    bind:value
    {...editorProps}
    {disabled}
    suggestionListId={open ? listId : undefined}
    activeSuggestionId={open ? `${listId}-${selectedIndex}` : undefined}
    extensions={[extension]}
    onfocus={(event) => { focused = true; onfocus?.(event) }}
  />
  <Popover.Root {open} onOpenChange={(next) => { if (!next) dismiss() }}>
    <Popover.Content
      customAnchor={anchor}
      trapFocus={false}
      side={commandSide}
      align="start"
      class="w-[min(28rem,calc(100vw-3rem))] gap-0 p-1"
      onOpenAutoFocus={(event) => event.preventDefault()}
      onCloseAutoFocus={(event) => event.preventDefault()}
      onInteractOutside={(event) => { if (anchor?.contains(event.target as Node)) event.preventDefault() }}
    >
      <div id={listId} role="listbox" aria-label="Chat commands">
        {#each matches as command, index (command.name)}
          <Button
            id={`${listId}-${index}`}
            variant="ghost"
            role="option"
            aria-selected={index === selectedIndex}
            disabled={Boolean(command.disabledReason)}
            tabindex={-1}
            class={`h-auto w-full justify-start gap-3 px-3 py-2 text-left ${index === selectedIndex ? 'bg-accent text-accent-foreground' : ''}`}
            onmouseenter={() => (selectedIndex = index)}
            onpointerdown={(event) => event.preventDefault()}
            onclick={() => choose(command)}
          >
            <span class="font-mono">{command.name}{command.name === '/rename' ? ' <name>' : ''}</span>
            <span class="whitespace-normal text-xs font-normal text-muted-foreground">{command.disabledReason ?? command.description}</span>
          </Button>
        {/each}
      </div>
    </Popover.Content>
  </Popover.Root>
</div>
