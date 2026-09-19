<script lang="ts">
  import { tick } from 'svelte'
  import ArrowsOutIcon from 'phosphor-svelte/lib/ArrowsOutIcon'
  import ArrowsInIcon from 'phosphor-svelte/lib/ArrowsInIcon'
  import StopIcon from 'phosphor-svelte/lib/StopIcon'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import FileMentionEditor from '$lib/components/file-picker/FileMentionEditor.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { promptValueAfterEnter } from '$lib/promptEnterBehavior'
  import type { FilePickerFileView } from '../../api/types'

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
    onSubmit: () => void
    onInterrupt?: () => void
    onFocus?: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = 'Continue the thread…',
    busy = false,
    workspaceId = null,
    inputId,
    fullscreen = false,
    submitLabel = 'Send',
    startSession = false,
    interruptMode = false,
    interruptBusy = false,
    autofocus = false,
    onSubmit,
    onInterrupt,
    onFocus,
  }: Props = $props()

  let fullscreenOpen = $state(false)
  const mentionIdentities = new Map<string, FilePickerFileView>()
  let fullscreenEditor = $state<{ focusEnd: () => void } | null>(null)

  async function openFullscreen(): Promise<void> {
    fullscreenOpen = true
    await tick()
    fullscreenEditor?.focusEnd()
  }

  function submit(): void {
    if (disabled || submitDisabled || busy) return
    onSubmit()
  }

  function submitAndCloseFullscreen(): void {
    if (disabled || submitDisabled || busy) return
    fullscreenOpen = false
    onSubmit()
  }

  function interrupt(closeFullscreen = false): void {
    if (interruptBusy || !onInterrupt) return
    if (closeFullscreen) fullscreenOpen = false
    onInterrupt()
  }

  function handleKeydown(event: KeyboardEvent): void {
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

<PromptInput.Root class="w-full" onSubmit={submit}>
  <PromptInput.Body>
    <div class="relative">
      <FileMentionEditor id={inputId} bind:value {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget {autofocus} onkeydown={handleKeydown} onfocus={onFocus} class={fullscreen ? 'min-h-[52px] px-4 py-3 pr-10 text-base sm:text-[13.5px]' : 'min-h-[52px] px-4 py-3 text-base sm:text-[13.5px]'} />
      {#if fullscreen}
        <Button type="button" variant="ghost" size="icon-sm" class="absolute right-1 top-1 sm:hidden" aria-label="Expand message composer" onclick={() => void openFullscreen()}>
          <ArrowsOutIcon class="size-4" />
        </Button>
      {/if}
    </div>
  </PromptInput.Body>
  <PromptInput.Toolbar class="justify-end pt-0">
    {#if interruptMode}
      <Button type="button" size="icon" disabled={interruptBusy} aria-label="Interrupt agent" title="Interrupt agent" onclick={() => interrupt()}>
        <StopIcon class="size-4" />
      </Button>
    {:else}
      <PromptInput.Submit disabled={disabled || submitDisabled} {busy} label={submitLabel} {startSession} />
    {/if}
  </PromptInput.Toolbar>
</PromptInput.Root>

{#if fullscreen}
  <Dialog.Root bind:open={fullscreenOpen}>
    <Dialog.Content class="inset-0 left-0 top-0 flex h-svh max-h-svh w-screen max-w-none translate-x-0 translate-y-0 flex-col overflow-hidden rounded-none p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:hidden" showCloseButton={false}>
      <Dialog.Header class="shrink-0">
        <div class="flex items-center justify-between gap-2">
          <Dialog.Title>Expanded message composer</Dialog.Title>
          <Button type="button" variant="ghost" size="icon-sm" aria-label="Close expanded message composer" onclick={() => (fullscreenOpen = false)}>
            <ArrowsInIcon class="size-4" />
          </Button>
        </div>
        <Dialog.Description>Write a longer follow-up message.</Dialog.Description>
      </Dialog.Header>

      <PromptInput.Root class="mt-2 flex min-h-0 w-full flex-1 flex-col shadow-none" onSubmit={submitAndCloseFullscreen}>
        <PromptInput.Body class="min-h-0 flex-1">
          <FileMentionEditor bind:this={fullscreenEditor} bind:value {workspaceId} {placeholder} {disabled} {mentionIdentities} shortcutFocusTarget autofocus onkeydown={handleKeydown} onfocus={onFocus} class="h-full min-h-0 pr-2" />
        </PromptInput.Body>
        <PromptInput.Toolbar class="shrink-0 justify-end pt-0">
          {#if interruptMode}
            <Button type="button" size="icon" disabled={interruptBusy} aria-label="Interrupt agent" title="Interrupt agent" onclick={() => interrupt(true)}>
              <StopIcon class="size-4" />
            </Button>
          {:else}
            <PromptInput.Submit disabled={disabled || submitDisabled} {busy} label={submitLabel} {startSession} />
          {/if}
        </PromptInput.Toolbar>
      </PromptInput.Root>
    </Dialog.Content>
  </Dialog.Root>
{/if}
