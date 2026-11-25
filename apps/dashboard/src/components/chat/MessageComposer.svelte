<script lang="ts">
  import { createComposer } from "../../features/chat/composer.svelte"
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import ResponsiveMenu from '$lib/components/ui/overlay/ResponsiveMenu.svelte'
  import { t } from '$lib/i18n';
  import { tick, type Snippet } from 'svelte'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  import ArrowsInIcon from 'phosphor-svelte/lib/ArrowsInIcon'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import StackSimpleIcon from 'phosphor-svelte/lib/StackSimpleIcon'
  import StopIcon from 'phosphor-svelte/lib/StopIcon'
  import NotePencilIcon from 'phosphor-svelte/lib/NotePencilIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import SignOutIcon from 'phosphor-svelte/lib/SignOutIcon'
  import PushPinIcon from 'phosphor-svelte/lib/PushPinIcon'
  import PushPinSlashIcon from 'phosphor-svelte/lib/PushPinSlashIcon'
  import ArchiveIcon from 'phosphor-svelte/lib/ArchiveIcon'
  import BoxArrowUpIcon from 'phosphor-svelte/lib/BoxArrowUpIcon'
  import DotsThreeVerticalIcon from 'phosphor-svelte/lib/DotsThreeVerticalIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import UploadAttachments from './UploadAttachments.svelte'
  import { useFileDrop } from './fileDrop.svelte'
  import ChatCommandInput from './ChatCommandInput.svelte'
  import { type ChatCommand } from '$lib/chatCommands'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'
  import type { FilePickerFileView, InboxDeliveryPolicy } from '../../api/types'


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
    prefix?: Snippet
    onRemovePrefix?: () => void
    draftId?: string
    retainDraft?: boolean
    onSubmit: (message: string, deliveryPolicy: InboxDeliveryPolicy) => void | Promise<boolean | void>
    onInterrupt?: () => void
    onFocus?: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = $t('chat.followUp'),
    busy = false,
    workspaceId = null,
    inputId,
    fullscreen = false,
    submitLabel = $t('common.send'),
    startSession = false,
    interruptMode = false,
    interruptBusy = false,
    autofocus = false,
    commands = [],
    header,
    prefix,
    onRemovePrefix,
    draftId,
    retainDraft = false,
    onSubmit,
    onInterrupt,
    onFocus,
  }: Props = $props()

  let fullscreenOpen = $state(false)
  const controller = createComposer({
    get value() { return value },
    set value(next) { value = next },
    get draftId() { return draftId },
    get retainDraft() { return retainDraft },
    get disabled() { return disabled },
    get busy() { return busy },
    get submitDisabled() { return submitDisabled },
    get commands() { return commands },
    get onSubmit() { return onSubmit },
    closeExpanded: () => { fullscreenOpen = false },
  })
  const command = $derived(controller.command)
  let inlineInput = $state<HTMLDivElement | null>(null)
  // Sticky until the draft is cleared, so a line that only wraps in the narrow compact row cannot flip layouts back and forth.
  let multiline = $state(false)
  const compact = $derived(fullscreen && !multiline)
  let uploadPicker = $state<{ choose: () => void } | null>(null)
  let fullscreenUploadPicker = $state<{ choose: () => void } | null>(null)
  function commandActionFor(item: ChatCommand): { label: string; icon: typeof NotePencilIcon } {
    switch (item.name) {
      case '/new': return { label: $t('chat.new'), icon: NotePencilIcon }
      case '/rename': return { label: $t('common.rename'), icon: PencilSimpleIcon }
      case '/exit': return { label: $t('common.exit'), icon: SignOutIcon }
      case '/reload': return { label: $t('bots.reload'), icon: ArrowClockwiseIcon }
      case '/compact': return { label: $t('compaction.action'), icon: StackSimpleIcon }
      case '/pin': return item.active ? { label: $t('sessions.unpin'), icon: PushPinSlashIcon } : { label: $t('sessions.pin'), icon: PushPinIcon }
      case '/archive': return item.active ? { label: $t('common.unarchive'), icon: BoxArrowUpIcon } : { label: $t('common.archive'), icon: ArchiveIcon }
    }
  }
  const mentionIdentities = new Map<string, FilePickerFileView>()
  let fullscreenEditor = $state<{ focusEnd: () => void } | null>(null)
  let editor = $state<{ focusEnd: () => void } | null>(null)
  let focusCommandInput = false
  const availableCommands = $derived(startSession ? [] : commands.filter((item) => !item.disabledReason))
  // Fixed menu groups.
  const menuGroups: { names: ChatCommand['name'][] }[] = [
    { names: ['/new'] },
    { names: ['/rename', '/pin', '/archive'] },
    { names: ['/compact'] },
    { names: ['/exit'] },
  ]
  const visibleMenuGroups = $derived(menuGroups
    .map(group => ({
      commands: group.names.flatMap(name => availableCommands.filter(item => item.name === name)),
    }))
    .filter(group => group.commands.length > 0))
  const commandAction = $derived(command ? commandActionFor(command) : undefined)
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

  export function focusInput(): void {
    (fullscreenOpen ? fullscreenEditor : editor)?.focusEnd()
  }

  async function openFullscreen(): Promise<void> {
    fullscreenOpen = true
    await tick()
    fullscreenEditor?.focusEnd()
  }

  export function submit(policy:InboxDeliveryPolicy='steer') {return controller.submit(policy)}
  function chooseMenuCommand(selected: ChatCommand): void {
    if (disabled || busy || selected.disabledReason) return
    if (selected.name === '/rename') {
      value = '/rename '
      focusCommandInput = true
      return
    }
    controller.executeCommand(selected)
  }

  function restoreCommandFocus(event: Event): void {
    if (!focusCommandInput) return
    event.preventDefault()
    focusCommandInput = false
    void tick().then(() => (fullscreenOpen ? fullscreenEditor : editor)?.focusEnd())
  }

  function submitAndCloseFullscreen(): void {
    if (controller.cannotSubmit) return
    fullscreenOpen = false
    submit()
  }

  function interrupt(closeFullscreen = false): void {
    if (interruptBusy || !onInterrupt) return
    if (closeFullscreen) fullscreenOpen = false
    onInterrupt()
  }

  // Office apps put a rendered image next to copied rich text; keep pasting that as text.
  function pasteFiles(event: ClipboardEvent): void {
    const data = event.clipboardData
    if (!controller.uploads || controller.uploadUnavailable || !data?.files.length || data.types.includes('text/rtf')) return
    event.preventDefault()
    event.stopPropagation()
    void controller.uploads.add(Array.from(data.files))
  }

  const fileDrop = useFileDrop()
  $effect(() => {
    const draft = controller.uploads
    if (!fileDrop || !draft) return
    return fileDrop.register({
      get unavailable() { return controller.uploadUnavailable },
      add: files => void draft.add(files),
    })
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing || event.keyCode === 229) return
    const queueInput = !event.shiftKey && !event.ctrlKey && !event.metaKey
      && ((event.key === 'Tab' && !event.altKey) || (event.key === 'Enter' && event.altKey))
    if (queueInput) {
      if (controller.cannotSubmit || command || (event.key === 'Tab' && value.trim() && commands.some(item => item.name.startsWith(value.trim())))) return
      event.preventDefault()
      submit('after_idle')
      return
    }
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
  {#if controller.uploads}
    <Button type="button" variant="ghost" size="icon-sm" aria-label={$t('files.upload')} title={$t('files.upload')} disabled={controller.uploadUnavailable} onclick={() => picker?.choose()}>
      <PlusIcon class="size-4" />
    </Button>
  {/if}
{/snippet}

{#snippet actions()}
  {#if visibleMenuGroups.length > 0}
    <div class="shrink-0">
      <ResponsiveMenu label={$t('chat.actions')} side="top" align="end" class="w-64" preventScroll={false} onCloseAutoFocus={restoreCommandFocus}>
        {#snippet trigger()}
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button {...props} type="button" variant="ghost" size="icon-sm" aria-label={$t('chat.actions')} title={$t('chat.actions')} disabled={disabled || busy}>
                <DotsThreeVerticalIcon class="size-5" />
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
        {/snippet}
        {#each visibleMenuGroups as group, index}
          {#if index > 0}<DropdownMenu.Separator />{/if}
          {#each group.commands as item (item.name)}
            {@const action = commandActionFor(item)}
            <DropdownMenu.Item variant={item.name === '/exit' ? 'destructive' : 'default'} onSelect={() => chooseMenuCommand(item)}>
              <action.icon class="size-4" />
              <span class="min-w-0 flex-1">
                <span class="block">{action.label}</span>
              </span>
              <span class="font-mono text-xs text-muted-foreground">{item.name}</span>
            </DropdownMenu.Item>
          {/each}
        {/each}
      </ResponsiveMenu>
    </div>
  {/if}
{/snippet}

{#if header || visibleMenuGroups.length > 0}
  <div class="mb-1 flex min-w-0 items-center justify-end gap-2">
    {@render header?.()}
    {@render actions()}
  </div>
{/if}

<PromptInput.Root class={compact ? 'grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center' : 'w-full'} onSubmit={() => submit()}>
  {#if controller.uploads}
    <div class={compact ? 'col-span-3 row-start-1' : 'contents'}>
      <UploadAttachments bind:this={uploadPicker} draft={controller.uploads} disabled={disabled || busy || controller.sending} />
    </div>
  {/if}
  <PromptInput.Body class={compact ? 'col-start-2 row-start-2' : undefined} onpastecapture={pasteFiles}>
    <div bind:this={inlineInput} class="relative">
      <ChatCommandInput bind:this={editor} id={inputId} bind:value {prefix} {onRemovePrefix} {commands} onCommand={controller.executeCommand} {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget {autofocus} onkeydown={handleKeydown} onfocus={onFocus} class={fullscreen ? `min-h-0 py-2 text-base sm:text-[13.5px] ${compact ? 'px-0' : 'pl-3 pr-10 sm:pr-3'}` : 'min-h-[52px] px-4 py-3 text-base sm:text-[13.5px]'} />
      {#if fullscreen && !compact}
        <Button type="button" variant="ghost" size="icon-sm" class="absolute right-1 top-1 sm:hidden" aria-label={$t('chat.expandComposer')} onclick={() => void openFullscreen()}>
          <ArrowsOutIcon class="size-4" />
        </Button>
      {/if}
    </div>
  </PromptInput.Body>
  <div class={compact ? 'col-span-3' : 'contents'}>
    {#if command?.disabledReason}<p role="status" class="px-4 pb-2 text-xs text-muted-foreground">{command.disabledReason}</p>{/if}
  </div>
  <PromptInput.Toolbar class={compact ? 'contents' : cn(fullscreen && 'px-1 pb-1', controller.uploads ? 'pt-0' : 'justify-end pt-0')}>
    <div class={compact ? 'col-start-1 row-start-2 py-1 pl-1' : 'contents'}>
      {@render uploadButton(uploadPicker)}
    </div>
    <div class={compact ? 'col-start-3 row-start-2 p-1' : 'contents'}>
      {#if interruptMode && controller.attachments.items.length === 0}
        <Button type="button" size={fullscreen ? 'icon-sm' : 'icon'} disabled={interruptBusy} aria-label={$t('chat.interrupt')} title={$t('chat.interrupt')} onclick={() => interrupt()}>
          <StopIcon class="size-4" />
        </Button>
      {:else}
        <PromptInput.Submit disabled={controller.cannotSubmit} {busy} label={controller.sentAwaitingCleanup ? $t('files.dismissSent') : commandAction?.label ?? submitLabel} icon={commandAction?.icon} startSession={startSession && !command} class={fullscreen ? cn('h-7 gap-1.5 px-3 text-xs font-semibold', compact && 'max-sm:w-7 max-sm:px-0') : undefined} labelClass={compact ? 'max-sm:sr-only' : undefined} />
      {/if}
    </div>
  </PromptInput.Toolbar>
</PromptInput.Root>

{#if fullscreen}
  <Dialog.Root bind:open={fullscreenOpen}>
    <ResponsiveDialog mode="fullscreen" class="overflow-hidden sm:hidden" showCloseButton={false}>
      <Dialog.Header class="shrink-0">
        <div class="flex items-center justify-between gap-2">
          <Dialog.Title>{$t('chat.expandedComposer')}</Dialog.Title>
          <div class="flex shrink-0 items-center gap-2">
            {@render actions()}
            <Button type="button" variant="ghost" size="icon-sm" class="touch-target:size-11" aria-label={$t('chat.closeExpandedComposer')} onclick={() => (fullscreenOpen = false)}>
              <ArrowsInIcon class="size-4" />
            </Button>
          </div>
        </div>
        <Dialog.Description>{$t('chat.longerFollowUp')}</Dialog.Description>
      </Dialog.Header>

      <PromptInput.Root class="mt-2 flex min-h-0 w-full flex-1 flex-col shadow-none" onSubmit={submitAndCloseFullscreen}>
        {#if controller.uploads}<UploadAttachments bind:this={fullscreenUploadPicker} draft={controller.uploads} disabled={disabled || busy || controller.sending} />{/if}
        <PromptInput.Body class="min-h-0 flex-1" onpastecapture={pasteFiles}>
          <ChatCommandInput bind:this={fullscreenEditor} bind:value {prefix} {onRemovePrefix} {commands} commandSide="bottom" onCommand={controller.executeCommand} {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget autofocus onkeydown={handleKeydown} onfocus={onFocus} class="h-full min-h-[52px] overscroll-contain pr-2" />
        </PromptInput.Body>
        {#if command?.disabledReason}<p role="status" class="pb-2 text-xs text-muted-foreground">{command.disabledReason}</p>{/if}
        <PromptInput.Toolbar class={controller.uploads ? 'shrink-0 pt-0' : 'shrink-0 justify-end pt-0'}>
          {@render uploadButton(fullscreenUploadPicker)}
          {#if interruptMode && controller.attachments.items.length === 0}
            <Button type="button" size="icon" disabled={interruptBusy} aria-label={$t('chat.interrupt')} title={$t('chat.interrupt')} onclick={() => interrupt(true)}>
              <StopIcon class="size-4" />
            </Button>
          {:else}
            <PromptInput.Submit disabled={controller.cannotSubmit} {busy} label={controller.sentAwaitingCleanup ? $t('files.dismissSent') : commandAction?.label ?? submitLabel} icon={commandAction?.icon} startSession={startSession && !command} />
          {/if}
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </ResponsiveDialog>
  </Dialog.Root>
{/if}
