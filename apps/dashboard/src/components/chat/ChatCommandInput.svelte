<script lang="ts">
  import { tick, type ComponentProps } from 'svelte'
  import FileMentionEditor from '$lib/components/file-picker/FileMentionEditor.svelte'
  import * as Popover from '$lib/components/ui/popover/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { chatCommandQuery, type ChatCommand } from '$lib/chatCommands'

  let {
    value = $bindable(''),
    commands,
    onCommand,
    onkeydown,
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
  let dismissedQuery = $state<string | null>(null)
  let selectedIndex = $state(0)
  const query = $derived(chatCommandQuery(value))
  const matches = $derived(query === null ? [] : commands.filter((command) => command.name.startsWith(query)))
  const open = $derived(focused && !disabled && query !== null && query !== dismissedQuery && matches.length > 0)

  $effect(() => {
    // A changed query starts a fresh selection, including after Escape.
    query
    selectedIndex = 0
    dismissedQuery = null
  })

  export function focusEnd(): void {
    editor?.focusEnd()
  }

  function execute(command: ChatCommand): void {
    if (disabled || command.disabledReason) return
    if (command.name === '/rename') {
      value = '/rename '
      void tick().then(focusEnd)
      return
    }
    dismissedQuery = query
    onCommand(command)
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing || event.keyCode === 229) return
    if (open && !event.shiftKey && !event.ctrlKey && !event.metaKey && !event.altKey) {
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        selectedIndex = (selectedIndex + (event.key === 'ArrowDown' ? 1 : -1) + matches.length) % matches.length
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        dismissedQuery = query
        return
      }
      const command = matches[selectedIndex]
      if (command && (event.key === 'Enter' || event.key === 'Tab')) {
        event.preventDefault()
        if (event.key === 'Tab') {
          value = command.name === '/rename' ? '/rename ' : command.name
          void tick().then(focusEnd)
        } else {
          execute(command)
        }
        return
      }
    }
    onkeydown?.(event)
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
    onkeydown={handleKeydown}
    onfocus={(event) => { focused = true; onfocus?.(event) }}
  />
  <Popover.Root {open} onOpenChange={(next) => { if (!next) dismissedQuery = query }}>
    <Popover.Content
      customAnchor={anchor}
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
            onclick={() => execute(command)}
          >
            <span class="font-mono">{command.name}{command.name === '/rename' ? ' <name>' : ''}</span>
            <span class="whitespace-normal text-xs font-normal text-muted-foreground">{command.disabledReason ?? command.description}</span>
          </Button>
        {/each}
      </div>
    </Popover.Content>
  </Popover.Root>
</div>
