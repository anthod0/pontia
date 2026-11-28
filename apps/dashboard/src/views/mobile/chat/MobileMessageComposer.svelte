<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { FilePickerFileView } from '../../../api/types'
  import ChatCommandInput from '../../../components/chat/ChatCommandInput.svelte'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createComposer } from '../../../features/chat/composer.svelte'
  import type { ChatCommand } from '$lib/chatCommands'
  import UploadAttachments from '../../../components/chat/UploadAttachments.svelte'
  import StopIcon from 'phosphor-svelte/lib/StopIcon'
  import ComposerAddMenu from '../../../components/chat/ComposerAddMenu.svelte'
  import PaperPlaneTiltIcon from 'phosphor-svelte/lib/PaperPlaneTiltIcon'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  let {
    value = $bindable(''),
    prefix,
    onRemovePrefix,
    draftId,
    workspaceId,
    autofocus = false,
    commands,
    busy,
    submitDisabled,
    onInterrupt,
    onSubmit,
  }: {
    value: string
    autofocus?: boolean
    workspaceId?: string
    prefix?: Snippet
    onRemovePrefix?: () => void
    draftId: string
    commands: ChatCommand[]
    busy: boolean
    submitDisabled: boolean
    onInterrupt?: () => void
    onSubmit: (text: string) => Promise<boolean>
  } = $props()
  const stack = useMobileStack()!
  let expanded = $state(false)
  const controller = createComposer({
    get value() {
      return value
    },
    set value(next) {
      value = next
    },
    get draftId() {
      return draftId
    },
    get retainDraft() {
      return workspaceId !== undefined
    },
    get commands() {
      return commands
    },
    get busy() {
      return busy
    },
    get submitDisabled() {
      return submitDisabled
    },
    onSubmit: (text) => onSubmit(text),
    closeExpanded: () => {
      if (expanded) stack.back()
    },
  })
  const mentionIdentities = new Map<string, FilePickerFileView>()
  const hasPrefix = $derived(Boolean(prefix))
  const placeholder = $derived(workspaceId !== undefined ? $t('chat.taskPlaceholder') : $t('bots.message'))
  let inlineInput = $state<ChatCommandInput>()
  let expandedInput = $state<ChatCommandInput>()
  let picker = $state<UploadAttachments>()
  let height = $state(0)
  export function focusInput() {
    (expanded ? expandedInput : inlineInput)?.focusEnd()
  }
  export function submit() {
    return controller.submit()
  }
  function paste(event: ClipboardEvent) {
    const data = event.clipboardData
    if (!data?.files.length || data.types.includes('text/rtf') || controller.uploadUnavailable) return
    event.preventDefault()
    void controller.uploads?.add(Array.from(data.files))
  }
  function holdFocus(event: MouseEvent) {
    event.preventDefault()
  }
</script>

{#snippet editor()}
  <div class="flex h-full flex-col gap-2">
    {#if prefix}<div class="mx-3 mt-2">{@render prefix()}</div>{/if}
    <div class="min-h-0 flex-1 overflow-auto [&_[role=listbox]]:static" onpastecapture={paste}>
      <ChatCommandInput
        bind:this={expandedInput}
        bind:value
        {hasPrefix}
        {onRemovePrefix}
        {workspaceId}
        {mentionIdentities}
        {commands}
        onCommand={controller.executeCommand}
        commandSide="bottom"
        autofocus
        {placeholder}
        class="min-h-32 max-h-none p-3 text-base"
      />
    </div>
    <button
      class="min-h-11"
      disabled={controller.cannotSubmit}
      onclick={() => {
        if (submit() && !controller.command) stack.back()
      }}>{$t('common.send')}</button
    >
  </div>
{/snippet}
{#if controller.uploads}<UploadAttachments
    bind:this={picker}
    draft={controller.uploads}
    disabled={busy || controller.sending}
  />{/if}
{#if controller.submissionError}
  <div role="alert" class="mx-4 mb-1 flex items-center gap-2 rounded-item border border-destructive/30 bg-destructive/10 px-3 py-2 text-13 text-destructive">
    <WarningCircleIcon class="size-4 shrink-0" weight="fill" />
    <span class="min-w-0 flex-1 break-words">{controller.submissionError}</span>
    <button class="min-h-11 shrink-0 px-2 font-medium" onclick={() => controller.retrySubmission()}>{$t('common.retry')}</button>
    <button class="grid size-11 shrink-0 place-items-center" aria-label={$t('common.close')} onclick={() => controller.dismissSubmissionError()}><XIcon class="size-4" /></button>
  </div>
{/if}
<div class="mobile-glass flex flex-col rounded-composer p-1">
  {#if prefix}<div class="ml-12 mr-2 mt-2">{@render prefix()}</div>{/if}
  <div class="flex items-end">
    <ComposerAddMenu mobile disabled={!!controller.uploadUnavailable} onSelectFile={() => picker?.choose()} onConfirm={({value,label}) => controller.uploads?.addSecret(value,label)} onClose={focusInput}/>
  <div class="min-w-0 flex-1 [&_[role=listbox]]:max-w-full" bind:clientHeight={height} onpastecapture={paste}>
    <ChatCommandInput
      bind:this={inlineInput}
      bind:value
      {hasPrefix}
      {onRemovePrefix}
      {workspaceId}
      {mentionIdentities}
      {commands}
      onCommand={controller.executeCommand}
      {autofocus}
      {placeholder}
      class="max-h-36 px-1 py-2.5 text-base leading-6"
    />
  </div>
  {#if height > 72}<button
      class="mobile-icon"
      aria-label={$t('chat.expandComposer')}
      onclick={() => {
        expanded = true
        stack.open({
          content: editor,
          title: $t('chat.expandedComposer'),
          modal: true,
          dismiss: () => (expanded = false),
        })
      }}><ArrowsOutIcon class="size-4" /></button
    >{/if}
  {#if onInterrupt && !value.trim() && !controller.attachments.items.length}
    <button class="mobile-icon bg-destructive text-white" aria-label={$t('chat.interrupt')} disabled={busy} onmousedown={holdFocus} onclick={onInterrupt}><StopIcon class="size-4" weight="fill"/></button>
  {:else}
  <button
    class="mobile-icon shrink-0 bg-primary text-primary-foreground disabled:opacity-40"
    disabled={controller.cannotSubmit}
    aria-label={$t('common.send')}
    onmousedown={holdFocus}
    onclick={() => submit()}><PaperPlaneTiltIcon class="size-4" /></button
  >
  {/if}
  </div>
</div>
