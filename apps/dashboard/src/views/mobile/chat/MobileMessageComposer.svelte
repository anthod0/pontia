<script lang="ts">
  import type { FilePickerFileView } from '../../../api/types'
  import FileMentionEditor from '$lib/components/file-picker/FileMentionEditor.svelte'
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createComposer } from '../../../features/chat/composer.svelte'
  import type { ChatCommand } from '$lib/chatCommands'
  import UploadAttachments from '../../../components/chat/UploadAttachments.svelte'
  import StopIcon from 'phosphor-svelte/lib/StopIcon'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import PaperPlaneTiltIcon from 'phosphor-svelte/lib/PaperPlaneTiltIcon'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  let {
    value = $bindable(''),
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
  let mentionEditor = $state<FileMentionEditor>()
  let input = $state<HTMLTextAreaElement>()
  let picker = $state<UploadAttachments>()
  let height = $state(0)
  export function focusInput() {
    if (mentionEditor) mentionEditor.focusEnd()
    else input?.focus()
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
    {#if workspaceId !== undefined}
      <div class="min-h-0 flex-1 overflow-auto" onpastecapture={paste}>
        <FileMentionEditor
          bind:value
          {workspaceId}
          {mentionIdentities}
          autofocus
          placeholder={$t('chat.taskPlaceholder')}
          class="min-h-32 p-3 text-base"
        />
      </div>
    {:else}
      <textarea
        onpaste={paste}
        bind:value
        aria-label={$t('bots.message')}
        class="min-h-0 w-full flex-1 resize-none rounded-xl border p-3 text-base"
        {@attach (node) => node.focus()}></textarea>
    {/if}
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
<div class="mobile-glass flex items-end rounded-3xl p-1">
  <button
    class="mobile-icon shrink-0"
    aria-label={$t('files.upload')}
    disabled={controller.uploadUnavailable}
    onmousedown={holdFocus}
    onclick={() => picker?.choose()}><PlusIcon class="size-4" /></button
  >
  {#if workspaceId !== undefined}
    <div class="min-w-0 flex-1" bind:clientHeight={height} onpastecapture={paste}>
      <FileMentionEditor
        bind:this={mentionEditor}
        {mentionIdentities}
        bind:value
        {autofocus}
        {workspaceId}
        placeholder={$t('chat.taskPlaceholder')}
        class="max-h-36 overflow-auto px-1 py-2 text-base"
      />
    </div>
  {:else}
    <textarea
      onpaste={paste}
      bind:this={input}
      bind:value
      bind:clientHeight={height}
      rows="1"
      aria-label={$t('bots.message')}
      placeholder={$t('bots.message')}
      class="my-2 min-w-0 flex-1 resize-none bg-transparent text-base leading-6 outline-none [field-sizing:content] max-h-36"
    ></textarea>
  {/if}
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
