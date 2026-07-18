<script lang="ts">
  import { Maximize2, Minimize2 } from '@lucide/svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import FileMentionTextarea from '$lib/components/file-picker/FileMentionTextarea.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'

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
    onSubmit: () => void
    onFocus?: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = 'Send a follow-up message…',
    busy = false,
    workspaceId = null,
    inputId,
    fullscreen = false,
    submitLabel,
    onSubmit,
    onFocus,
  }: Props = $props()

  let fullscreenOpen = $state(false)

  function submit(): void {
    if (disabled || submitDisabled || busy) return
    onSubmit()
  }

  function submitAndCloseFullscreen(): void {
    if (disabled || submitDisabled || busy) return
    fullscreenOpen = false
    onSubmit()
  }

  function handleKeydown(event: KeyboardEvent): void {
    const isPlainEnter = event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey
    if (!isPlainEnter) return

    const nextValue = promptValueAfterEnter(value)
    if (nextValue !== null) {
      event.preventDefault()
      value = nextValue
      const textarea = event.currentTarget instanceof HTMLTextAreaElement ? event.currentTarget : null
      queueMicrotask(() => textarea?.setSelectionRange(nextValue.length, nextValue.length))
      return
    }

    event.preventDefault()
    submit()
  }
</script>

<PromptInput.Root class="w-full" onSubmit={submit}>
  <PromptInput.Body>
    <div class="relative">
      <FileMentionTextarea id={inputId} bind:value {workspaceId} {placeholder} {disabled} shortcutFocusTarget onkeydown={handleKeydown} onfocus={onFocus} class={fullscreen ? 'min-h-10 pr-10' : 'min-h-10'} />
      {#if fullscreen}
        <Button type="button" variant="ghost" size="icon-sm" class="absolute right-1 top-1 sm:hidden" aria-label="Expand message composer" onclick={() => (fullscreenOpen = true)}>
          <Maximize2 class="size-4" />
        </Button>
      {/if}
    </div>
  </PromptInput.Body>
  <PromptInput.Toolbar class="justify-end pt-0">
    <PromptInput.Submit disabled={disabled || submitDisabled} {busy} aria-label={submitLabel} />
  </PromptInput.Toolbar>
</PromptInput.Root>

{#if fullscreen}
  <Dialog.Root bind:open={fullscreenOpen}>
    <Dialog.Content class="inset-0 left-0 top-0 flex h-svh max-h-svh w-screen max-w-none translate-x-0 translate-y-0 flex-col overflow-hidden rounded-none p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:hidden" showCloseButton={false}>
      <Dialog.Header class="shrink-0">
        <div class="flex items-center justify-between gap-2">
          <Dialog.Title>Expanded message composer</Dialog.Title>
          <Button type="button" variant="ghost" size="icon-sm" aria-label="Close expanded message composer" onclick={() => (fullscreenOpen = false)}>
            <Minimize2 class="size-4" />
          </Button>
        </div>
        <Dialog.Description>Write a longer follow-up message.</Dialog.Description>
      </Dialog.Header>

      <PromptInput.Root class="mt-2 flex min-h-0 w-full flex-1 flex-col shadow-none" onSubmit={submitAndCloseFullscreen}>
        <PromptInput.Body class="min-h-0 flex-1">
          <FileMentionTextarea bind:value {workspaceId} {placeholder} {disabled} shortcutFocusTarget onkeydown={handleKeydown} onfocus={onFocus} class="h-full min-h-0 pr-2" />
        </PromptInput.Body>
        <PromptInput.Toolbar class="shrink-0 justify-end pt-0">
          <PromptInput.Submit disabled={disabled || submitDisabled} {busy} aria-label={submitLabel} />
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </Dialog.Content>
  </Dialog.Root>
{/if}
