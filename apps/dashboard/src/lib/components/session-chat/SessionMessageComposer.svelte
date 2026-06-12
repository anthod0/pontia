<script lang="ts">
  import * as PromptInput from '$lib/components/ai-elements/prompt-input/index.js'

  interface Props {
    value: string
    disabled?: boolean
    submitDisabled?: boolean
    placeholder?: string
    busy?: boolean
    onValueChange: (value: string) => void
    onSubmit: () => void
  }

  let {
    value = $bindable(''),
    disabled = false,
    submitDisabled = false,
    placeholder = 'Send a follow-up message…',
    busy = false,
    onValueChange,
    onSubmit,
  }: Props = $props()

  $effect(() => {
    onValueChange(value)
  })

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      if (!disabled && !busy) onSubmit()
    }
  }
</script>

<PromptInput.Root class="w-full" {onSubmit}>
  <PromptInput.Body>
    <PromptInput.Textarea bind:value {placeholder} {disabled} onkeydown={handleKeydown} class="h-10 min-h-10 md:h-auto md:min-h-20" />
  </PromptInput.Body>
  <PromptInput.Toolbar class="justify-between">
    <p class="px-2 text-xs text-muted-foreground">Enter to send · Shift+Enter for newline</p>
    <PromptInput.Submit disabled={disabled || submitDisabled || busy} />
  </PromptInput.Toolbar>
</PromptInput.Root>
