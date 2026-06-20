<script lang="ts">
  import { Maximize2, Minimize2 } from '@lucide/svelte'
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'

  interface Props {
    value: string
    disabled?: boolean
    submitDisabled?: boolean
    placeholder?: string
    busy?: boolean
    onValueChange: (value: string) => void
    onSubmit: () => void
    onFocus?: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = 'Send a follow-up message…',
    busy = false,
    onValueChange,
    onSubmit,
    onFocus,
  }: Props = $props()

  let expanded = $state(false)
  let fullscreenOpen = $state(false)

  $effect(() => {
    onValueChange(value)
  })

  function isMobileViewport(): boolean {
    if (typeof window === 'undefined') return false
    return window.matchMedia?.('(max-width: 639px)').matches ?? window.innerWidth < 640
  }

  function toggleComposerSize(): void {
    if (isMobileViewport()) {
      fullscreenOpen = true
      return
    }
    expanded = !expanded
  }

  function submitAndCloseFullscreen(): void {
    if (disabled || submitDisabled || busy) return
    fullscreenOpen = false
    onSubmit()
  }

  function handleKeydown(event: KeyboardEvent) {
    const shouldSubmit = event.key === 'Enter' && (event.shiftKey || event.ctrlKey || event.metaKey)
    if (shouldSubmit) {
      event.preventDefault()
      if (!disabled && !submitDisabled && !busy) onSubmit()
    }
  }
</script>

<PromptInput.Root class="w-full" {onSubmit}>
  <PromptInput.Body>
    <div class="relative">
      <PromptInput.Textarea bind:value {placeholder} {disabled} onkeydown={handleKeydown} onfocus={onFocus} class={`h-10 min-h-10 pr-10 md:h-auto ${expanded ? 'md:min-h-56' : 'md:min-h-20'}`} />
      <Button type="button" variant="ghost" size="icon-sm" class="absolute right-1 top-1" aria-label={expanded ? 'Collapse message composer' : 'Expand message composer'} onclick={toggleComposerSize}>
        {#if expanded}<Minimize2 class="size-4" />{:else}<Maximize2 class="size-4" />{/if}
      </Button>
    </div>
  </PromptInput.Body>
  <PromptInput.Toolbar class="justify-between">
    <p class="px-2 text-xs text-muted-foreground">Shift+Enter / Ctrl+Enter to send · Enter for newline</p>
    <PromptInput.Submit disabled={disabled || submitDisabled || busy} />
  </PromptInput.Toolbar>
</PromptInput.Root>

<Dialog.Root bind:open={fullscreenOpen}>
  <Dialog.Content class="inset-0 left-0 top-0 h-dvh max-h-dvh w-screen max-w-none translate-x-0 translate-y-0 rounded-none p-4 sm:hidden" showCloseButton={false}>
    <Dialog.Header>
      <div class="flex items-center justify-between gap-2">
        <Dialog.Title>Expanded message composer</Dialog.Title>
        <Button type="button" variant="ghost" size="icon-sm" aria-label="Close expanded message composer" onclick={() => (fullscreenOpen = false)}>
          <Minimize2 class="size-4" />
        </Button>
      </div>
      <Dialog.Description>Write a longer follow-up message.</Dialog.Description>
    </Dialog.Header>

    <PromptInput.Root class="mt-2 w-full shadow-none" onSubmit={submitAndCloseFullscreen}>
      <PromptInput.Body>
        <PromptInput.Textarea bind:value {placeholder} {disabled} onkeydown={handleKeydown} onfocus={onFocus} class="min-h-[calc(100vh-12rem)] pr-2" />
      </PromptInput.Body>
      <PromptInput.Toolbar class="justify-between">
        <p class="px-2 text-xs text-muted-foreground">Shift+Enter / Ctrl+Enter to send · Enter for newline</p>
        <PromptInput.Submit disabled={disabled || submitDisabled || busy} />
      </PromptInput.Toolbar>
    </PromptInput.Root>
  </Dialog.Content>
</Dialog.Root>
