<script lang="ts">
  import { useDeviceRuntime } from '../../device/context';
  import { tick, type Snippet } from 'svelte'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  import ArrowsInIcon from 'phosphor-svelte/lib/ArrowsInIcon'
  import StackSimpleIcon from 'phosphor-svelte/lib/StackSimpleIcon'
  import StopIcon from 'phosphor-svelte/lib/StopIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import CpuIcon from 'phosphor-svelte/lib/CpuIcon'
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import UploadAttachments from './UploadAttachments.svelte'
  import { uploadMessage } from '$lib/uploads/message'
  import { type UploadDraftState } from "$lib/uploads/draft";
  import ChatCommandInput from './ChatCommandInput.svelte'
  import { findChatCommand, type ChatCommand } from '$lib/chatCommands'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'
  import type { FilePickerFileView } from '../../api/types'
  const device = useDeviceRuntime();
  const { createUploadDraft } = device.uploadDrafts;

  interface Props {
    value: string
    disabled?: boolean
    submitDisabled?: boolean
    placeholder?: string
    busy?: boolean
    workspaceId?: string | null
    inputId?: string
    fullscreen?: boolean
    submitLabel?: string
    startSession?: boolean
    interruptMode?: boolean
    interruptBusy?: boolean
    autofocus?: boolean
    commands?: ChatCommand[]
    header?: Snippet
    menuItems?: Snippet
    draftId?: string
    onSubmit: (message: string) => void | Promise<boolean | void>
    onInterrupt?: () => void
    onFocus?: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = 'Follow up…',
    busy = false,
    workspaceId = null,
    inputId,
    fullscreen = false,
    submitLabel = 'Send',
    startSession = false,
    interruptMode = false,
    interruptBusy = false,
    autofocus = false,
    commands = [],
    header,
    menuItems,
    draftId,
    onSubmit,
    onInterrupt,
    onFocus,
  }: Props = $props()

  let attachments = $state<UploadDraftState>({ ready: false, items: [], warning: null })
  let uploads = $state<ReturnType<typeof createUploadDraft> | null>(null)
  $effect(() => {
    if (!draftId) return
    const draft = createUploadDraft(draftId)
    uploads = draft
    const unsubscribe = draft.subscribe(state => { attachments = state })
    return () => { unsubscribe(); draft.destroy() }
  })
  let sentAwaitingCleanup = $state(false)
  let sending = $state(false)
  let submissionError = $state<string | null>(null)
  const uploadBlocked = $derived(Boolean(draftId) && (!attachments.ready || attachments.items.some(item => item.status !== 'complete')))
  const completedFiles = $derived(attachments.items.flatMap(item => item.status === 'complete' && item.upload?.path ? [{ name: item.name, path: item.upload.path }] : []))
  let fullscreenOpen = $state(false)
  let inlineInput = $state<HTMLDivElement | null>(null)
  // Sticky until the draft is cleared, so a line that only wraps in the narrow compact row cannot flip layouts back and forth.
  let multiline = $state(false)
  const compact = $derived(fullscreen && !multiline)
  let uploadPicker = $state<{ choose: () => void } | null>(null)
  let fullscreenUploadPicker = $state<{ choose: () => void } | null>(null)
  const commandActions: Record<ChatCommand['name'], { label: string; icon: typeof NotePencilIcon }> = {
    '/new': { label: 'New chat', icon: NotePencilIcon },
    '/rename': { label: 'Rename', icon: PencilSimpleIcon },
    '/exit': { label: 'Exit', icon: SignOutIcon },
    '/model': { label: 'Choose model', icon: CpuIcon },
    '/compact': { label: 'Compact context', icon: StackSimpleIcon },
  }
  const mentionIdentities = new Map<string, FilePickerFileView>()
  let fullscreenEditor = $state<{ focusEnd: () => void } | null>(null)
  let editor = $state<{ focusEnd: () => void } | null>(null)
  let focusCommandInput = false
  const availableCommands = $derived(startSession ? [] : commands.filter((item) => !item.disabledReason))
  const command = $derived(findChatCommand(value, commands))
  const commandAction = $derived(command ? commandActions[command.name] : undefined)
  const commandArgument = $derived(command ? value.trim().slice(command.name.length).trim() : '')
  const cannotSubmit = $derived(disabled || busy || sending || (command ? Boolean(command.disabledReason) || (command.name === '/rename' && !commandArgument) : submitDisabled || uploadBlocked || (!value.trim() && completedFiles.length === 0)))

  $effect(() => {
    if (!value) multiline = false
  })

  $effect(() => {
    const wrapper = inlineInput
    if (!fullscreen || !wrapper) return
    const observer = new ResizeObserver(() => {
      const textbox = wrapper.querySelector<HTMLElement>('[role="textbox"]')
      if (multiline || !textbox) return
      const style = getComputedStyle(textbox)
      const contentHeight = textbox.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom)
      if (contentHeight > parseFloat(style.lineHeight) * 1.5) multiline = true
    })
    observer.observe(wrapper)
    return () => observer.disconnect()
  })

  async function openFullscreen(): Promise<void> {
    fullscreenOpen = true
    await tick()
    fullscreenEditor?.focusEnd()
  }

  function submit(): void {
    if (cannotSubmit) return
    if (command) executeCommand(command)
    else void submitMessage()
  }

  async function submitMessage(): Promise<void> {
    const draft = uploads
    sending = true
    submissionError = null
    try {
      if (!sentAwaitingCleanup) sentAwaitingCleanup = (await onSubmit(uploadMessage(value, completedFiles))) === true
      if (sentAwaitingCleanup) {
        await draft?.clear()
        sentAwaitingCleanup = false
      }
    } catch (error) {
      submissionError = error instanceof Error ? error.message : String(error)
    } finally {
      sending = false
    }
  }

  function executeCommand(command: ChatCommand): void {
    if (disabled || busy || command.disabledReason) return
    if (command.name === '/rename') {
      if (!commandArgument) return
      fullscreenOpen = false
      command.run(commandArgument)
      return
    }
    value = command.name
    fullscreenOpen = false
    command.run()
  }

  function chooseMenuCommand(selected: ChatCommand): void {
    if (disabled || busy || selected.disabledReason) return
    if (selected.name === '/rename') {
      value = '/rename '
      focusCommandInput = true
      return
    }
    executeCommand(selected)
  }

  function restoreCommandFocus(event: Event): void {
    if (!focusCommandInput) return
    event.preventDefault()
    focusCommandInput = false
    void tick().then(() => (fullscreenOpen ? fullscreenEditor : editor)?.focusEnd())
  }

  function submitAndCloseFullscreen(): void {
    if (cannotSubmit) return
    fullscreenOpen = false
    submit()
  }

  function interrupt(closeFullscreen = false): void {
    if (interruptBusy || !onInterrupt) return
    if (closeFullscreen) fullscreenOpen = false
    onInterrupt()
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing || event.keyCode === 229) return
    const isPlainEnter = event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey
    if (!isPlainEnter) return

    const nextValue = promptValueAfterEnter(value)
    if (nextValue !== null) {
      event.preventDefault()
      value = nextValue
      return
    }

    event.preventDefault()
    submit()
  }
</script>

{#snippet uploadButton(picker: { choose: () => void } | null)}
  {#if uploads}
    <Button type="button" variant="ghost" size="icon-sm" aria-label="Upload files" title="Upload files" disabled={disabled || busy || sending || !attachments.ready} onclick={() => picker?.choose()}>
      <PlusIcon class="size-4" />
    </Button>
  {/if}
{/snippet}

{#snippet actions()}
  {#if menuItems || availableCommands.length > 0}
    <div class="shrink-0">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} type="button" variant="ghost" size="icon-sm" aria-label="Chat actions" title="Chat actions" disabled={disabled || busy}>
              <DotsThreeVerticalIcon class="size-5" />
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content side="top" align="end" class="w-64" preventScroll={false} onCloseAutoFocus={restoreCommandFocus}>
          {#if menuItems}
            {@render menuItems()}
            {#if availableCommands.length > 0}<DropdownMenu.Separator />{/if}
          {/if}
          {#each availableCommands as item (item.name)}
            {@const action = commandActions[item.name]}
            <DropdownMenu.Item variant={item.name === '/exit' ? 'destructive' : 'default'} onSelect={() => chooseMenuCommand(item)}>
              <action.icon class="size-4" />
              <span class="min-w-0 flex-1">
                <span class="block">{action.label}</span>
              </span>
              <span class="font-mono text-xs text-muted-foreground">{item.name}</span>
            </DropdownMenu.Item>
          {/each}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  {/if}
{/snippet}

{#if header || menuItems || availableCommands.length > 0}
  <div class="mb-1 flex min-w-0 items-center justify-end gap-2">
    {@render header?.()}
    {@render actions()}
  </div>
{/if}

<PromptInput.Root class={compact ? 'grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center' : 'w-full'} onSubmit={submit}>
  <PromptInput.Body class={compact ? 'col-start-2 row-start-1' : undefined}>
    <div bind:this={inlineInput} class="relative">
      <ChatCommandInput bind:this={editor} id={inputId} bind:value {commands} onCommand={executeCommand} {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget {autofocus} onkeydown={handleKeydown} onfocus={onFocus} class={fullscreen ? `min-h-0 py-2 text-base sm:text-[13.5px] ${compact ? 'px-0' : 'pl-3 pr-10 sm:pr-3'}` : 'min-h-[52px] px-4 py-3 text-base sm:text-[13.5px]'} />
      {#if fullscreen && !compact}
        <Button type="button" variant="ghost" size="icon-sm" class="absolute right-1 top-1 sm:hidden" aria-label="Expand message composer" onclick={() => void openFullscreen()}>
          <ArrowsOutIcon class="size-4" />
        </Button>
      {/if}
    </div>
  </PromptInput.Body>
  <div class={compact ? 'col-span-3' : 'contents'}>
    {#if uploads}<UploadAttachments bind:this={uploadPicker} draft={uploads} disabled={disabled || busy || sending} />{/if}
    {#if submissionError}<p role="alert" class="px-4 text-xs text-destructive">{submissionError}</p>{/if}
    {#if command?.disabledReason}<p role="status" class="px-4 pb-2 text-xs text-muted-foreground">{command.disabledReason}</p>{/if}
  </div>
  <PromptInput.Toolbar class={compact ? 'contents' : cn(fullscreen && 'px-1 pb-1', uploads ? 'pt-0' : 'justify-end pt-0')}>
    <div class={compact ? 'col-start-1 row-start-1 py-1 pl-1' : 'contents'}>
      {@render uploadButton(uploadPicker)}
    </div>
    <div class={compact ? 'col-start-3 row-start-1 p-1' : 'contents'}>
      {#if interruptMode && attachments.items.length === 0}
        <Button type="button" size={fullscreen ? 'icon-sm' : 'icon'} disabled={interruptBusy} aria-label="Interrupt agent" title="Interrupt agent" onclick={() => interrupt()}>
          <StopIcon class="size-4" />
        </Button>
      {:else}
        <PromptInput.Submit disabled={cannotSubmit} {busy} label={sentAwaitingCleanup ? 'Dismiss sent attachments' : commandAction?.label ?? submitLabel} icon={commandAction?.icon} startSession={startSession && !command} class={fullscreen ? cn('h-7 gap-1.5 px-3 text-xs font-semibold', compact && 'max-sm:w-7 max-sm:px-0') : undefined} labelClass={compact ? 'max-sm:sr-only' : undefined} />
      {/if}
    </div>
  </PromptInput.Toolbar>
</PromptInput.Root>

{#if fullscreen}
  <Dialog.Root bind:open={fullscreenOpen}>
    <Dialog.Content class="inset-0 left-0 top-0 flex h-svh max-h-svh w-screen max-w-none translate-x-0 translate-y-0 flex-col overflow-hidden rounded-none p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:hidden" showCloseButton={false}>
      <Dialog.Header class="shrink-0">
        <div class="flex items-center justify-between gap-2">
          <Dialog.Title>Expanded message composer</Dialog.Title>
          <div class="flex shrink-0 items-center gap-2">
            {@render actions()}
            <Button type="button" variant="ghost" size="icon-sm" aria-label="Close expanded message composer" onclick={() => (fullscreenOpen = false)}>
              <ArrowsInIcon class="size-4" />
            </Button>
          </div>
        </div>
        <Dialog.Description>Write a longer follow-up message.</Dialog.Description>
      </Dialog.Header>

      <PromptInput.Root class="mt-2 flex min-h-0 w-full flex-1 flex-col shadow-none" onSubmit={submitAndCloseFullscreen}>
        <PromptInput.Body class="min-h-0 flex-1">
          <ChatCommandInput bind:this={fullscreenEditor} bind:value {commands} commandSide="bottom" onCommand={executeCommand} {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget autofocus onkeydown={handleKeydown} onfocus={onFocus} class="h-full min-h-[52px] pr-2" />
        </PromptInput.Body>
        {#if command?.disabledReason}<p role="status" class="pb-2 text-xs text-muted-foreground">{command.disabledReason}</p>{/if}
        {#if uploads}<UploadAttachments bind:this={fullscreenUploadPicker} draft={uploads} disabled={disabled || busy || sending} />{/if}
        <PromptInput.Toolbar class={uploads ? 'shrink-0 pt-0' : 'shrink-0 justify-end pt-0'}>
          {@render uploadButton(fullscreenUploadPicker)}
          {#if interruptMode && attachments.items.length === 0}
            <Button type="button" size="icon" disabled={interruptBusy} aria-label="Interrupt agent" title="Interrupt agent" onclick={() => interrupt(true)}>
              <StopIcon class="size-4" />
            </Button>
          {:else}
            <PromptInput.Submit disabled={cannotSubmit} {busy} label={sentAwaitingCleanup ? 'Dismiss sent attachments' : commandAction?.label ?? submitLabel} icon={commandAction?.icon} startSession={startSession && !command} />
          {/if}
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </Dialog.Content>
  </Dialog.Root>
{/if}
