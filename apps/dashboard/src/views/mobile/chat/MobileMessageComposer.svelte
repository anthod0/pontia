<script lang="ts">
  import { t } from '$lib/i18n'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { createComposer } from '../../../features/chat/composer.svelte'
  import type { ChatCommand } from '$lib/chatCommands'
  import UploadAttachments from '../../../components/chat/UploadAttachments.svelte'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import PaperPlaneTiltIcon from 'phosphor-svelte/lib/PaperPlaneTiltIcon'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  let {
    value = $bindable(''),
    draftId,
    commands,
    busy,
    submitDisabled,
    onSubmit,
  }: {
    value: string
    draftId: string
    commands: ChatCommand[]
    busy: boolean
    submitDisabled: boolean
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
  let input = $state<HTMLTextAreaElement>()
  let picker = $state<UploadAttachments>()
  let height = $state(0)
  export function focusInput() {
    input?.focus()
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
  <textarea
    onpaste={paste}
    bind:value
    aria-label={$t('bots.message')}
    class="min-h-0 w-full flex-1 resize-none rounded-xl border p-3 text-base"
    {@attach (node) => node.focus()}></textarea>
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
    onclick={() => picker?.choose()}><PlusIcon /></button
  >
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
      }}><ArrowsOutIcon /></button
    >{/if}
  <button
    class="mobile-icon shrink-0 bg-primary text-primary-foreground disabled:opacity-40"
    disabled={controller.cannotSubmit}
    aria-label={$t('common.send')}
    onmousedown={holdFocus}
    onclick={() => submit()}><PaperPlaneTiltIcon /></button
  >
</div>
